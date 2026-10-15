//! The PipeWire side of Hepcat. Runs its own thread with a PipeWire main loop,
//! keeps a [`Graph`] mirror up to date, re-applies the user's routing whenever
//! the graph changes, and publishes a JSON snapshot for the UI.

mod format;
mod fx;
pub mod graph;
mod meter;
mod volume;

use crate::config::{BUS_PREFIX, BusConfig, Config, Effects, FxTarget};
use fx::{FxNames, FxNode};
use graph::{Graph, Kind, LinkInfo, NodeInfo, PortInfo, PortPair};
use pipewire as pw;
use pw::metadata::{Metadata, MetadataListener};
use pw::node::{Node, NodeListener};
use pw::properties::properties;
use pw::registry::RegistryRc;
use pw::spa;
use pw::spa::pod::{Pod, Value, ValueArray, deserialize::PodDeserializer};
use pw::types::ObjectType;
use serde_json::json;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};
use meter::Meter;
use volume::{Change, Route};

/// Requests from the UI.
#[derive(Debug)]
pub enum Command {
    SetVolume { ids: Vec<u32>, volume: f64 },
    SetMute { ids: Vec<u32>, mute: bool },
    ToggleRoute { key: String, bus: String },
    SetBusOutput { bus: String, output: Option<String> },
    SetBusMic { bus: String, enabled: bool },
    AddBus { label: String },
    RemoveBus { name: String },
    SetDefaultSink { name: String },
    SetDefaultSource { name: String },
    /// Replace the effects of the strip with this key (`mic:…`, `app:…`, `bus:…`).
    SetFx { key: String, fx: Effects },
    /// Meters cost a little CPU, so they are paused while nobody is looking.
    SetMeters(bool),
}

pub type Sender = pw::channel::Sender<Command>;

/// Updates for the UI, both as JSON.
pub enum Event {
    /// Full snapshot of strips, sent whenever something visible changes.
    State(String),
    /// `{"<node id>": level}` for every meter, sent about 25 times a second
    /// while anything is moving.
    Levels(String),
}

/// Start the PipeWire thread. `on_event` is called on that thread.
pub fn spawn(on_event: impl Fn(Event) + Send + 'static) -> Sender {
    let (tx, rx) = pw::channel::channel();
    std::thread::Builder::new()
        .name("hepcat-pipewire".into())
        .spawn(move || {
            if let Err(e) = run(rx, Box::new(on_event)) {
                eprintln!("hepcat: PipeWire thread failed: {e:#}");
            }
        })
        .expect("failed to spawn PipeWire thread");
    tx
}

/// How long to wait for a link we requested to show up before asking again.
const LINK_RETRY: Duration = Duration::from_secs(3);

struct State {
    core: pw::core::CoreRc,
    registry: RegistryRc,
    graph: Graph,
    config: Option<Config>,
    ready: bool,
    dirty: bool,
    nodes: HashMap<u32, (Node, NodeListener)>,
    metadata: Option<(Metadata, MetadataListener)>,
    /// Bus nodes we asked PipeWire to create, by node.name, until they appear.
    pending_buses: HashMap<String, Node>,
    pending_links: HashMap<PortPair, (pw::link::Link, Instant)>,
    /// Non-lingering links (meters) live only as long as we hold their proxy.
    held_links: HashMap<u32, pw::link::Link>,
    destroyed: HashMap<u32, Instant>,
    /// target.object values we last wrote, so we only write changes.
    targets: HashMap<u32, String>,
    /// Bound audio devices, for setting volume on their routes.
    devices: HashMap<u32, (pw::device::Device, pw::device::DeviceListener)>,
    /// Active route of each (device id, card.profile.device).
    routes: HashMap<(u32, i32), Route>,
    /// Level meters by metered node id.
    meters: HashMap<u32, Meter>,
    meters_enabled: bool,
    on_event: Box<dyn Fn(Event)>,
    /// Buses and mics left behind by builds from when the app was called Mixwire.
    legacy_nodes: HashMap<u32, String>,
    /// Clients of a still-running old Mixwire; while any exist, its nodes stay.
    legacy_clients: HashSet<u32>,
    /// The user's chosen default output (may point at a node that is gone).
    configured_sink: Option<String>,
    configured_source: Option<String>,
    /// Effects nodes by strip key.
    fx: HashMap<String, FxNode>,
    /// A processed mic to make the default input as soon as it exists.
    pending_default_source: Option<String>,
    last_snapshot: String,
    last_levels: String,
}

fn run(rx: pw::channel::Receiver<Command>, on_event: Box<dyn Fn(Event)>) -> anyhow::Result<()> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(Some(properties! {
        "application.name" => "Hepcat",
        // Marks us as an audio manager (like pavucontrol): inside a Flatpak,
        // WirePlumber then lets us move streams and change defaults.
        "media.category" => "Manager",
    }))?;
    let registry = core.get_registry_rc()?;

    let state = Rc::new(RefCell::new(State {
        core: core.clone(),
        registry: registry.clone(),
        graph: Graph::default(),
        config: Config::load(),
        ready: false,
        dirty: true,
        nodes: HashMap::new(),
        metadata: None,
        pending_buses: HashMap::new(),
        pending_links: HashMap::new(),
        held_links: HashMap::new(),
        destroyed: HashMap::new(),
        targets: HashMap::new(),
        devices: HashMap::new(),
        routes: HashMap::new(),
        meters: HashMap::new(),
        meters_enabled: std::env::var_os("HEPCAT_NO_METERS").is_none(),
        on_event,
        legacy_nodes: HashMap::new(),
        legacy_clients: HashSet::new(),
        configured_sink: None,
        configured_source: None,
        fx: HashMap::new(),
        pending_default_source: None,
        last_snapshot: String::new(),
        last_levels: String::new(),
    }));

    let _registry_listener = registry
        .add_listener_local()
        .global({
            let weak = Rc::downgrade(&state);
            move |global| {
                if let Some(state) = weak.upgrade() {
                    on_global(&state, global);
                }
            }
        })
        .global_remove({
            let weak = Rc::downgrade(&state);
            move |id| {
                if let Some(state) = weak.upgrade() {
                    state.borrow_mut().on_global_remove(id);
                }
            }
        })
        .register();

    // Two round trips: the first delivers every existing global, the second
    // everything those globals emit once bound (node props, default sink).
    // Only then is it safe to decide which buses are missing.
    let first_sync = Rc::new(RefCell::new(Some(core.sync(0)?)));
    let _core_listener = core
        .add_listener_local()
        .done({
            let weak = Rc::downgrade(&state);
            let core = core.downgrade();
            let first_sync = first_sync.clone();
            let second_sync = RefCell::new(None);
            move |id, seq| {
                if id != pw::core::PW_ID_CORE {
                    return;
                }
                if first_sync.borrow().as_ref() == Some(&seq) {
                    first_sync.borrow_mut().take();
                    if let Some(core) = core.upgrade() {
                        *second_sync.borrow_mut() = core.sync(0).ok();
                    }
                } else if second_sync.borrow().as_ref() == Some(&seq) {
                    if let Some(state) = weak.upgrade() {
                        let mut st = state.borrow_mut();
                        st.ready = true;
                        st.dirty = true;
                    }
                }
            }
        })
        .error(|id, seq, res, message| {
            eprintln!("hepcat: PipeWire error on object {id} (seq {seq}, {res}): {message}");
        })
        .register();

    let _commands = rx.attach(mainloop.loop_(), {
        let weak = Rc::downgrade(&state);
        move |cmd| {
            if let Some(state) = weak.upgrade() {
                state.borrow_mut().handle(cmd);
            }
        }
    });

    // Graph events arrive in bursts (a new app brings a node, ports and links
    // one by one), so changes are applied on a short tick instead of per event.
    let timer = mainloop.loop_().add_timer({
        let weak = Rc::downgrade(&state);
        move |_| {
            if let Some(state) = weak.upgrade() {
                state.borrow_mut().tick();
            }
        }
    });
    let tick = Duration::from_millis(40);
    timer.update_timer(Some(tick), Some(tick)).into_result()?;

    mainloop.run();
    Ok(())
}

fn on_global(state: &Rc<RefCell<State>>, global: &pw::registry::GlobalObject<&spa::utils::dict::DictRef>) {
    let Some(props) = global.props else { return };
    let get = |k: &str| props.get(k).map(str::to_owned);
    let num = |k: &str| props.get(k).and_then(|v| v.parse::<u32>().ok());
    let mut st = state.borrow_mut();

    match global.type_ {
        ObjectType::Node => {
            if let Some(name) = props.get("node.name").filter(|n| n.starts_with(graph::LEGACY_PREFIX)) {
                // Old buses and mics are replaced by new ones; meters die with their client.
                if !name.contains(".meter.") {
                    st.legacy_nodes.insert(global.id, name.to_owned());
                    st.dirty = true;
                }
                return;
            }
            let class = get("media.class").unwrap_or_default();
            let mut info = NodeInfo {
                id: global.id,
                serial: get("object.serial").unwrap_or_default(),
                name: get("node.name").unwrap_or_default(),
                description: get("node.description").unwrap_or_default(),
                media_class: class,
                ..Default::default()
            };
            match info.kind() {
                None => return,
                // Our own capture streams only need to be known for linking.
                Some(Kind::Meter | Kind::FxIn | Kind::FxRef | Kind::FxOut) => {
                    st.graph.nodes.insert(global.id, info);
                    st.dirty = true;
                    return;
                }
                Some(_) => {}
            }
            let node: Node = match st.registry.bind(global) {
                Ok(node) => node,
                Err(e) => {
                    eprintln!("hepcat: failed to bind node {}: {e}", global.id);
                    return;
                }
            };
            let listener = node_listener(&node, global.id, Rc::downgrade(state));
            node.subscribe_params(&[spa::param::ParamType::Props]);
            info.volume = 1.0;
            st.pending_buses.remove(&info.name);
            st.graph.nodes.insert(global.id, info);
            st.nodes.insert(global.id, (node, listener));
        }
        ObjectType::Port => {
            if !props.get("format.dsp").is_some_and(|f| f.contains("audio")) {
                return;
            }
            let (Some(node), Some(dir)) = (num("node.id"), props.get("port.direction")) else { return };
            st.graph.ports.insert(
                global.id,
                PortInfo {
                    id: global.id,
                    node,
                    output: dir == "out",
                    monitor: props.get("port.monitor") == Some("true"),
                    channel: get("audio.channel").unwrap_or_default(),
                },
            );
        }
        ObjectType::Link => {
            let (Some(out_node), Some(out_port), Some(in_node), Some(in_port)) = (
                num("link.output.node"),
                num("link.output.port"),
                num("link.input.node"),
                num("link.input.port"),
            ) else {
                return;
            };
            let pair = PortPair { out_node, out_port, in_node, in_port };
            if let Some((proxy, _)) = st.pending_links.remove(&pair) {
                if holds_links(st.graph.nodes.get(&in_node)) {
                    st.held_links.insert(global.id, proxy);
                }
            }
            st.graph
                .links
                .insert(global.id, LinkInfo { id: global.id, out_node, out_port, in_node, in_port });
        }
        ObjectType::Device => {
            if props.get("media.class") != Some("Audio/Device") {
                return;
            }
            let device: pw::device::Device = match st.registry.bind(global) {
                Ok(device) => device,
                Err(e) => {
                    eprintln!("hepcat: failed to bind device {}: {e}", global.id);
                    return;
                }
            };
            let id = global.id;
            let weak = Rc::downgrade(state);
            let listener = device
                .add_listener_local()
                .param(move |_seq, _type, _index, _next, pod| {
                    let Some(state) = weak.upgrade() else { return };
                    if let Some((profile_device, route)) = pod.and_then(volume::parse_route) {
                        state.borrow_mut().routes.insert((id, profile_device), route);
                    }
                })
                .register();
            device.subscribe_params(&[spa::param::ParamType::Route]);
            st.devices.insert(id, (device, listener));
            return;
        }
        ObjectType::Client => {
            if props.get("application.name") == Some("Mixwire") {
                st.legacy_clients.insert(global.id);
            }
            return;
        }
        ObjectType::Metadata => {
            if props.get("metadata.name") != Some("default") {
                return;
            }
            let Ok(metadata) = st.registry.bind::<Metadata, _>(global) else { return };
            let weak = Rc::downgrade(state);
            let listener = metadata
                .add_listener_local()
                .property(move |subject, key, _type, value| {
                    let Some(state) = weak.upgrade() else { return 0 };
                    let mut st = state.borrow_mut();
                    match key {
                        Some("default.audio.sink") if subject == 0 => {
                            st.graph.default_sink = value.and_then(json_name);
                        }
                        Some("default.configured.audio.sink") if subject == 0 => {
                            st.configured_sink = value.and_then(json_name);
                        }
                        Some("default.audio.source") if subject == 0 => {
                            st.graph.default_source = value.and_then(json_name);
                        }
                        Some("default.configured.audio.source") if subject == 0 => {
                            st.configured_source = value.and_then(json_name);
                        }
                        Some("target.object") => match value {
                            Some(v) => {
                                st.graph.stream_targets.insert(subject, v.to_owned());
                            }
                            None => {
                                st.graph.stream_targets.remove(&subject);
                            }
                        },
                        // A `None` key means the whole metadata was cleared.
                        None => {
                            st.graph.default_sink = None;
                            st.graph.default_source = None;
                            st.graph.stream_targets.clear();
                        }
                        _ => return 0,
                    }
                    st.dirty = true;
                    0
                })
                .register();
            st.metadata = Some((metadata, listener));
        }
        _ => return,
    }
    st.dirty = true;
}

fn node_listener(node: &Node, id: u32, weak: Weak<RefCell<State>>) -> NodeListener {
    let weak2 = weak.clone();
    node.add_listener_local()
        .info(move |info| {
            // Info events for state changes carry an empty props dict; only a
            // PROPS change has real properties. Losing application.name would
            // change the app's route key and silently drop its routing.
            if !info.change_mask().contains(pw::node::NodeChangeMask::PROPS) {
                return;
            }
            let Some(state) = weak.upgrade() else { return };
            let Some(props) = info.props() else { return };
            let mut st = state.borrow_mut();
            let Some(node) = st.graph.nodes.get_mut(&id) else { return };
            let get = |k: &str| props.get(k).map(str::to_owned);
            let keep = |new: Option<String>, old: &mut Option<String>| {
                if new.is_some() {
                    *old = new;
                }
            };
            keep(get("application.name"), &mut node.app_name);
            keep(get("application.process.binary"), &mut node.binary);
            keep(get("application.icon-name").or_else(|| get("device.icon-name")), &mut node.icon);
            keep(get("media.name"), &mut node.media_name);
            let num = |k: &str| props.get(k).and_then(|v| v.parse().ok());
            if let (Some(device), Some(profile_device)) = (num("device.id"), num("card.profile.device")) {
                node.device = Some((device as u32, profile_device));
            }
            if let Some(d) = get("node.description") {
                node.description = d;
            }
            st.dirty = true;
        })
        .param(move |_seq, _type, _index, _next, pod| {
            let Some(state) = weak2.upgrade() else { return };
            let Some((volume, mute)) = pod.and_then(parse_props) else { return };
            let mut st = state.borrow_mut();
            let Some(node) = st.graph.nodes.get_mut(&id) else { return };
            if let Some((v, channels)) = volume {
                node.volume = v;
                node.channels = channels;
            }
            if let Some(m) = mute {
                node.mute = m;
            }
            st.dirty = true;
        })
        .register()
}

/// Pull channel volume (converted to the cubic UI scale, with the channel
/// count) and mute out of a node's Props param.
fn parse_props(pod: &Pod) -> Option<(Option<(f32, usize)>, Option<bool>)> {
    let (_, value) = PodDeserializer::deserialize_any_from(pod.as_bytes()).ok()?;
    let Value::Object(obj) = value else { return None };
    let mut volume = None;
    let mut mute = None;
    for prop in obj.properties {
        match (prop.key, prop.value) {
            (spa::sys::SPA_PROP_channelVolumes, Value::ValueArray(ValueArray::Float(v))) => {
                volume = v.iter().copied().reduce(f32::max).map(|max| (max.cbrt(), v.len()));
            }
            (spa::sys::SPA_PROP_mute, Value::Bool(m)) => mute = Some(m),
            _ => {}
        }
    }
    (volume.is_some() || mute.is_some()).then_some((volume, mute))
}

/// Links into our own capture streams die with Hepcat (they are not left
/// lingering), so we keep their proxies.
fn holds_links(target: Option<&NodeInfo>) -> bool {
    matches!(target.and_then(NodeInfo::kind), Some(Kind::Meter | Kind::FxIn | Kind::FxRef | Kind::FxDevice))
}

/// Metadata values for default devices look like `{"name":"alsa_output..."}`.
fn json_name(value: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(value).ok()?.get("name")?.as_str().map(str::to_owned)
}

impl State {
    fn on_global_remove(&mut self, id: u32) {
        self.graph.nodes.remove(&id);
        self.graph.ports.remove(&id);
        self.graph.links.remove(&id);
        self.nodes.remove(&id);
        if self.devices.remove(&id).is_some() {
            self.routes.retain(|(device, _), _| *device != id);
        }
        self.legacy_nodes.remove(&id);
        if self.legacy_clients.remove(&id) {
            self.dirty = true;
        }
        self.meters.remove(&id);
        self.held_links.remove(&id);
        self.targets.remove(&id);
        self.graph.stream_targets.remove(&id);
        self.destroyed.remove(&id);
        self.dirty = true;
    }

    fn tick(&mut self) {
        if !self.ready {
            return;
        }
        self.emit_levels();
        if !self.dirty {
            return;
        }
        self.dirty = false;
        if self.config.is_none() {
            let cfg = Config::first_run(self.graph.default_sink.clone());
            cfg.save();
            self.config = Some(cfg);
        }
        self.remove_legacy_nodes();
        self.ensure_buses();
        self.sync_fx();
        self.sync_meters();
        self.apply_plan();
        self.remember_output_labels();
        self.emit();
    }

    fn config(&mut self) -> &mut Config {
        self.config.get_or_insert_with(Config::default)
    }

    fn save(&mut self) {
        self.config().save();
        self.dirty = true;
    }

    /// Create missing buses and bus microphones, and remove bus microphones
    /// that were switched off.
    fn ensure_buses(&mut self) {
        let buses = self.config().buses.clone();
        for bus in &buses {
            let mut wanted = vec![(bus.node_name(), false)];
            if bus.mic {
                wanted.push((bus.mic_node_name(), true));
            }
            for (node_name, is_mic) in wanted {
                if self.graph.node_by_name(&node_name).is_some() || self.pending_buses.contains_key(&node_name) {
                    continue;
                }
                let created = if is_mic { create_bus_mic(&self.core, bus) } else { create_bus(&self.core, bus) };
                match created {
                    Ok(node) => {
                        self.pending_buses.insert(node_name, node);
                    }
                    Err(e) => eprintln!("hepcat: failed to create {node_name}: {e}"),
                }
            }
        }
        let unwanted: Vec<u32> = self
            .graph
            .nodes_of(Kind::BusMic)
            .filter(|n| !buses.iter().any(|b| b.mic && b.mic_node_name() == n.name))
            .map(|n| n.id)
            .collect();
        let now = Instant::now();
        for id in unwanted {
            if self.destroyed.insert(id, now).is_none() {
                let _ = self.registry.destroy_global(id);
            }
        }
    }

    /// Start, update and stop the effects nodes to match the config.
    fn sync_fx(&mut self) {
        let wanted: Vec<(String, Effects)> = self
            .config
            .iter()
            .flat_map(|c| c.fx.iter())
            .filter(|(_, fx)| fx.enabled)
            .map(|(key, fx)| (key.clone(), *fx))
            .collect();
        self.fx.retain(|key, _| wanted.iter().any(|(k, _)| k == key));
        for (key, fx) in wanted {
            if let Some(node) = self.fx.get_mut(&key) {
                node.update(&self.core, &fx);
                continue;
            }
            // Started once what they process shows up, and kept while it is
            // gone: apps stay attached to the virtual device, and browsers
            // that recreate their streams do not restart the effects.
            let Some(label) = self.fx_label(&key) else { continue };
            match FxNode::new(&self.core, &key, &label, &fx) {
                Ok(node) => {
                    self.fx.insert(key, node);
                }
                Err(e) => eprintln!("hepcat: failed to start effects for {key}: {e}"),
            }
        }

        if let Some(name) = &self.pending_default_source {
            if self.graph.node_by_name(name).is_some() {
                let name = name.clone();
                self.set_default_source(&name);
                self.pending_default_source = None;
            }
        }
    }

    /// Name of the strip with this effects key, if it is there right now.
    fn fx_label(&self, key: &str) -> Option<String> {
        let g = &self.graph;
        match FxTarget::parse(key)? {
            FxTarget::Mic(name) => g.node_by_name(name).filter(|n| n.kind() == Some(Kind::Mic)).map(NodeInfo::label),
            FxTarget::App(_) => g.nodes_of(Kind::App).find(|n| n.route_key().as_deref() == Some(key)).map(NodeInfo::label),
            FxTarget::Bus(name) => {
                g.node_by_name(&format!("{BUS_PREFIX}{name}"))?;
                self.config.as_ref()?.bus(name).map(|b| b.label.clone())
            }
        }
    }

    /// Stream nodes of the app with this route key.
    fn app_nodes(&self, key: &str) -> Vec<u32> {
        self.graph.nodes.values().filter(|n| n.route_key().as_deref() == Some(key)).map(|n| n.id).collect()
    }

    /// Let WirePlumber send these app streams to the default output again.
    fn clear_targets(&mut self, nodes: &[u32]) {
        for &id in nodes {
            self.targets.remove(&id);
            if let Some((metadata, _)) = &self.metadata {
                metadata.set_property(id, "target.object", None, None);
            }
        }
    }

    fn set_default_source(&mut self, name: &str) {
        if let Some((metadata, _)) = &self.metadata {
            let value = json!({ "name": name }).to_string();
            metadata.set_property(0, "default.configured.audio.source", Some("Spa:String:JSON"), Some(&value));
        }
    }

    /// Store a strip's effects. Whoever used a raw mic as the default input
    /// gets the processed one while effects are on, and the raw one back after.
    fn set_fx(&mut self, key: String, fx: Effects) {
        let Some(target) = FxTarget::parse(&key) else {
            eprintln!("hepcat: effects for unknown strip {key:?}");
            return;
        };
        let mut fx = fx.sanitized();
        // Echo cancellation removes the speakers from a microphone; nothing else has an echo.
        fx.echo &= matches!(target, FxTarget::Mic(_));
        let was_enabled = self.config().active_fx(&key).is_some();
        match target {
            FxTarget::Mic(mic) => {
                let device = FxNames::new(&key).device;
                let default = self.configured_source.clone().or(self.graph.default_source.clone());
                if fx.enabled && !was_enabled && default.as_deref() == Some(mic) {
                    self.pending_default_source = Some(device);
                } else if !fx.enabled && was_enabled {
                    if self.pending_default_source.as_deref() == Some(device.as_str()) {
                        self.pending_default_source = None;
                    }
                    if default.as_deref() == Some(device.as_str()) {
                        self.set_default_source(mic);
                    }
                }
            }
            // An app with a route is moved back to its first bus by the
            // reconciler; one without has to be handed back to WirePlumber.
            FxTarget::App(_) if was_enabled && !fx.enabled && !self.config().routes.contains_key(&key) => {
                let nodes = self.app_nodes(&key);
                self.clear_targets(&nodes);
            }
            _ => {}
        }
        self.config().fx.insert(key, fx);
        self.save();
    }

    fn remove_legacy_nodes(&mut self) {
        if !self.legacy_clients.is_empty() {
            return;
        }
        // If an old bus was the default output, make its replacement the default.
        let replacement = self
            .configured_sink
            .as_deref()
            .and_then(|s| s.strip_prefix(graph::LEGACY_PREFIX))
            .map(|rest| format!("hepcat.{rest}"))
            .filter(|name| self.graph.node_by_name(name).is_some());
        if let (Some(name), Some((metadata, _))) = (replacement, &self.metadata) {
            let value = json!({ "name": name }).to_string();
            metadata.set_property(0, "default.configured.audio.sink", Some("Spa:String:JSON"), Some(&value));
            self.configured_sink = Some(name);
        }
        let now = Instant::now();
        for (&id, name) in &self.legacy_nodes {
            if self.destroyed.insert(id, now).is_none() {
                eprintln!("hepcat: removing {name} left over from Mixwire");
                let _ = self.registry.destroy_global(id);
            }
        }
    }

    fn sync_meters(&mut self) {
        if !self.meters_enabled {
            self.meters.clear();
            return;
        }
        let wanted: Vec<u32> = self
            .graph
            .nodes
            .values()
            .filter(|n| matches!(n.kind(), Some(Kind::App | Kind::Mic | Kind::Bus | Kind::Output)))
            .map(|n| n.id)
            .collect();
        self.meters.retain(|id, _| wanted.contains(id));
        for id in wanted {
            if self.meters.contains_key(&id) {
                continue;
            }
            match Meter::new(&self.core, id) {
                Ok(meter) => {
                    self.meters.insert(id, meter);
                }
                Err(e) => eprintln!("hepcat: failed to create level meter for {id}: {e}"),
            }
        }
    }

    fn emit_levels(&mut self) {
        let round = |level: f32| json!((f64::from(level) * 100.0).round() / 100.0);
        let mut levels: serde_json::Map<String, serde_json::Value> =
            self.meters.iter_mut().map(|(id, m)| (id.to_string(), round(m.take_level()))).collect();
        // Processed mics are measured inside the processor, so their meter
        // moves even while no app is recording them.
        if self.meters_enabled {
            for (key, node) in &mut self.fx {
                let level = node.take_level();
                if let Some(output) = self.graph.node_by_name(&FxNames::new(key).output) {
                    levels.insert(output.id.to_string(), round(level));
                }
            }
        }
        let levels = serde_json::Value::Object(levels).to_string();
        if levels != self.last_levels {
            if std::env::var_os("HEPCAT_DEBUG").is_some() {
                eprintln!("hepcat: levels {levels}");
            }
            (self.on_event)(Event::Levels(levels.clone()));
            self.last_levels = levels;
        }
    }

    fn remember_output_labels(&mut self) {
        let Some(config) = &mut self.config else { return };
        let mut changed = false;
        for bus in &mut config.buses {
            let label = bus.output.as_deref().and_then(|o| self.graph.node_by_name(o)).map(NodeInfo::label);
            if label.is_some() && label != bus.output_label {
                bus.output_label = label;
                changed = true;
            }
        }
        if changed {
            config.save();
        }
    }

    fn apply_plan(&mut self) {
        let Some(config) = &self.config else { return };
        let plan = self.graph.plan(config);
        let now = Instant::now();

        self.pending_links.retain(|_, (_, at)| now.duration_since(*at) < LINK_RETRY);
        for pair in plan.create {
            if self.pending_links.contains_key(&pair) {
                continue;
            }
            let target = self.graph.nodes.get(&pair.in_node);
            let meter_of = target.filter(|n| n.kind() == Some(Kind::Meter));
            let held = holds_links(target);
            // Meters and the echo reference watch without keeping anything awake.
            let passive = matches!(target.and_then(NodeInfo::kind), Some(Kind::Meter | Kind::FxRef));
            // A meter we just dropped can still be in the graph for a moment.
            if let Some(meter) = meter_of {
                let ours = meter.metered_node().and_then(|id| self.meters.get(&id)).map(Meter::node_id);
                if ours != Some(meter.id) {
                    continue;
                }
            }
            let mut props = properties! {
                "link.output.node" => pair.out_node.to_string(),
                "link.output.port" => pair.out_port.to_string(),
                "link.input.node" => pair.in_node.to_string(),
                "link.input.port" => pair.in_port.to_string(),
            };
            if passive {
                props.insert("link.passive", "true");
            }
            if !held {
                // Keep routing alive when the window is closed.
                props.insert("object.linger", "true");
            }
            if std::env::var_os("HEPCAT_DEBUG").is_some() {
                eprintln!("hepcat: link {pair:?}");
            }
            let link = self.core.create_object::<pw::link::Link>("link-factory", &props);
            match link {
                Ok(link) => {
                    self.pending_links.insert(pair, (link, now));
                }
                Err(e) => eprintln!("hepcat: failed to create link {pair:?}: {e}"),
            }
        }

        self.destroyed.retain(|_, at| now.duration_since(*at) < LINK_RETRY);
        for id in plan.destroy {
            if self.destroyed.insert(id, now).is_none() {
                let _ = self.registry.destroy_global(id);
            }
        }

        for (stream, serial) in plan.targets {
            if self.targets.get(&stream) == Some(&serial) || self.graph.stream_targets.get(&stream) == Some(&serial) {
                continue;
            }
            if let Some((metadata, _)) = &self.metadata {
                metadata.set_property(stream, "target.object", Some("Spa:Id"), Some(&serial));
                self.targets.insert(stream, serial);
            }
        }
    }

    /// Set a node's volume or mute: on its device's active route for
    /// hardware, on the node itself otherwise.
    fn apply_volume(&self, id: u32, change: Change) {
        let Some(info) = self.graph.nodes.get(&id) else { return };
        let route = info.device.and_then(|(dev, pd)| Some((self.devices.get(&dev)?, pd, *self.routes.get(&(dev, pd))?)));
        if let Some(((device, _), profile_device, route)) = route {
            let route = Route { channels: if route.channels > 0 { route.channels } else { info.channels }, ..route };
            if let Some(bytes) = volume::route_param(route, profile_device, change) {
                if let Some(pod) = Pod::from_bytes(&bytes) {
                    device.set_param(spa::param::ParamType::Route, 0, pod);
                }
            }
            return;
        }
        let Some((node, _)) = self.nodes.get(&id) else { return };
        if let Some(bytes) = volume::node_param(change, info.channels) {
            if let Some(pod) = Pod::from_bytes(&bytes) {
                node.set_param(spa::param::ParamType::Props, 0, pod);
            }
        }
    }

    fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::SetVolume { ids, volume } => {
                for id in ids {
                    self.apply_volume(id, Change::Volume(volume));
                }
            }
            Command::SetMute { ids, mute } => {
                for id in ids {
                    self.apply_volume(id, Change::Mute(mute));
                }
            }
            Command::ToggleRoute { key, bus } => self.toggle_route(&key, &bus),
            Command::SetBusOutput { bus, output } => {
                let output_label = output.as_deref().and_then(|o| self.graph.node_by_name(o)).map(NodeInfo::label);
                if let Some(b) = self.config().buses.iter_mut().find(|b| b.name == bus) {
                    if b.output != output {
                        b.output_label = output_label;
                    }
                    b.output = output;
                }
                self.save();
            }
            Command::SetBusMic { bus, enabled } => {
                if let Some(b) = self.config().buses.iter_mut().find(|b| b.name == bus) {
                    b.mic = enabled;
                }
                self.save();
            }
            Command::AddBus { label } => {
                let label = label.trim().to_string();
                if label.is_empty() {
                    return;
                }
                let default = self.graph.default_sink.clone();
                let output = default.filter(|d| self.graph.node_by_name(d).and_then(NodeInfo::kind) == Some(Kind::Output));
                let name = self.config().unique_bus_name(&label);
                let output_label = output.as_deref().and_then(|o| self.graph.node_by_name(o)).map(NodeInfo::label);
                self.config().buses.push(BusConfig { name, label, output, output_label, mic: false });
                self.save();
            }
            Command::RemoveBus { name } => self.remove_bus(&name),
            Command::SetMeters(enabled) => {
                self.meters_enabled = enabled && std::env::var_os("HEPCAT_NO_METERS").is_none();
                self.dirty = true;
            }
            Command::SetDefaultSink { name } => {
                if let Some((metadata, _)) = &self.metadata {
                    let value = json!({ "name": name }).to_string();
                    metadata.set_property(0, "default.configured.audio.sink", Some("Spa:String:JSON"), Some(&value));
                }
            }
            Command::SetDefaultSource { name } => {
                self.pending_default_source = None;
                self.set_default_source(&name);
            }
            Command::SetFx { key, fx } => self.set_fx(key, fx),
        }
    }

    /// Buses the source with route key `key` is currently feeding, directly
    /// or through its effects, in bus order.
    fn current_buses(&self, key: &str, nodes: &[u32]) -> Vec<String> {
        let g = &self.graph;
        let fx = self.config.as_ref().and_then(|c| g.fx(c, key));
        let fx = fx.into_iter().flat_map(|f| [f.device, f.output]).flatten().map(|n| n.id);
        let linked: BTreeSet<u32> =
            nodes.iter().copied().chain(fx).flat_map(|n| self.graph.linked_targets(n)).collect();
        self.config
            .iter()
            .flat_map(|c| c.buses.iter())
            .filter(|b| self.graph.node_by_name(&b.node_name()).is_some_and(|n| linked.contains(&n.id)))
            .map(|b| b.name.clone())
            .collect()
    }

    fn toggle_route(&mut self, key: &str, bus: &str) {
        let nodes = self.app_nodes(key);
        let mut route = match self.config().routes.get(key) {
            Some(r) => r.clone(),
            None => self.current_buses(key, &nodes),
        };
        if let Some(pos) = route.iter().position(|b| b == bus) {
            route.remove(pos);
        } else {
            route.push(bus.to_string());
        }

        if route.is_empty() && key.starts_with("app:") {
            // No buses left: clearing the target makes WirePlumber move the app
            // back to the default device by itself. Its links must not be
            // destroyed here, or WirePlumber loses track of the stream. With
            // effects on, the app stays on them and they go to the default.
            self.config().routes.remove(key);
            if self.config().active_fx(key).is_none() {
                self.clear_targets(&nodes);
            }
        } else {
            self.config().routes.insert(key.to_string(), route);
        }
        self.save();
    }

    fn remove_bus(&mut self, name: &str) {
        let node_name = {
            let cfg = self.config();
            cfg.buses.retain(|b| b.name != name);
            for route in cfg.routes.values_mut() {
                route.retain(|b| b != name);
            }
            cfg.routes.retain(|k, r| !(r.is_empty() && k.starts_with("app:")));
            format!("{}{name}", crate::config::BUS_PREFIX)
        };
        if let Some(node) = self.graph.node_by_name(&node_name) {
            let _ = self.registry.destroy_global(node.id);
        }
        self.pending_buses.remove(&node_name);
        self.save();
    }

    fn emit(&mut self) {
        let snapshot = self.snapshot().to_string();
        if snapshot != self.last_snapshot {
            if std::env::var_os("HEPCAT_DEBUG").is_some() {
                eprintln!("hepcat: snapshot {snapshot}");
            }
            (self.on_event)(Event::State(snapshot.clone()));
            self.last_snapshot = snapshot;
        }
    }

    fn snapshot(&self) -> serde_json::Value {
        let g = &self.graph;
        let empty = Config::default();
        let cfg = self.config.as_ref().unwrap_or(&empty);
        let is_default = |name: &str| g.default_sink.as_deref() == Some(name);
        // Effects settings of a strip, the node playing its processed audio
        // (if running) and the ids to meter: with effects on, what is heard.
        let fx_info = |key: &str, ids: &[u32]| {
            let fx = cfg.fx.get(key).copied().unwrap_or_default();
            let output = g.fx(cfg, key).and_then(|f| f.output);
            let meter: Vec<u32> = output.map_or_else(|| ids.to_vec(), |o| vec![o.id]);
            (serde_json::to_string(&fx).unwrap_or_default(), output, meter)
        };

        // Apps are grouped by route key, so a browser with five tabs playing
        // shows up as one strip.
        let mut app_groups: Vec<(String, Vec<&NodeInfo>)> = vec![];
        for node in g.nodes_of(Kind::App) {
            let key = node.route_key().unwrap_or_default();
            match app_groups.iter_mut().find(|(k, _)| *k == key) {
                Some((_, nodes)) => nodes.push(node),
                None => app_groups.push((key, vec![node])),
            }
        }
        let apps: Vec<_> = app_groups
            .iter()
            .map(|(key, nodes)| {
                let ids: Vec<u32> = nodes.iter().map(|n| n.id).collect();
                let (fx, fx_output, meter) = fx_info(key, &ids);
                let direct: Vec<String> = ids
                    .iter()
                    .chain(fx_output.map(|o| &o.id))
                    .flat_map(|&id| g.linked_targets(id))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .filter_map(|id| g.nodes.get(&id))
                    .filter(|n| n.kind() == Some(Kind::Output))
                    .map(NodeInfo::label)
                    .collect();
                let first = nodes[0];
                json!({
                    "key": key,
                    "label": first.label(),
                    "detail": first.detail(),
                    "icon": first.icon.clone().or(first.binary.clone()).unwrap_or_default(),
                    "ids": ids,
                    "volume": first.volume,
                    "mute": first.mute,
                    "buses": self.current_buses(key, &ids),
                    "pinned": cfg.routes.contains_key(key),
                    "direct": direct.join(", "),
                    "meterIds": meter,
                    "fx": fx,
                    "fxActive": fx_output.is_some(),
                })
            })
            .collect();

        let mics: Vec<_> = g
            .nodes_of(Kind::Mic)
            .map(|n| {
                let key = n.route_key().unwrap_or_default();
                let (fx, fx_output, meter) = fx_info(&key, &[n.id]);
                let device = g.fx(cfg, &key).and_then(|f| f.device);
                let default = g.default_source.as_deref();
                json!({
                    "key": key,
                    "name": n.name,
                    "label": n.label(),
                    "icon": n.icon.clone().unwrap_or_else(|| "audio-input-microphone".into()),
                    "ids": [n.id],
                    "meterIds": meter,
                    "volume": n.volume,
                    "mute": n.mute,
                    "buses": self.current_buses(&key, &[n.id]),
                    "pinned": cfg.routes.contains_key(&key),
                    "isDefault": default.is_some() && (default == Some(n.name.as_str()) || default == device.map(|d| d.name.as_str())),
                    "fx": fx,
                    "fxActive": fx_output.is_some(),
                    // The processed mic, for making it the default input.
                    "fxDevice": device.map(|d| d.name.clone()).unwrap_or_default(),
                })
            })
            .collect();

        let buses: Vec<_> = cfg
            .buses
            .iter()
            .map(|b| {
                let node = g.node_by_name(&b.node_name());
                let output = b.output.as_deref().and_then(|o| g.node_by_name(o));
                let ids: Vec<u32> = node.map(|n| vec![n.id]).unwrap_or_default();
                let (fx, fx_output, meter) = fx_info(&format!("bus:{}", b.name), &ids);
                json!({
                    "name": b.name,
                    "label": b.label,
                    "ids": node.map(|n| vec![n.id]).unwrap_or_default(),
                    "online": node.is_some(),
                    "volume": node.map_or(1.0, |n| n.volume),
                    "mute": node.is_some_and(|n| n.mute),
                    "output": b.output.clone().unwrap_or_default(),
                    "outputLabel": output
                        .map(NodeInfo::label)
                        .or(b.output_label.clone())
                        .or(b.output.clone())
                        .unwrap_or_default(),
                    "outputOnline": output.is_some(),
                    "isDefault": is_default(&b.node_name()),
                    "nodeName": b.node_name(),
                    "mic": b.mic,
                    "meterIds": meter,
                    "fx": fx,
                    "fxActive": fx_output.is_some(),
                })
            })
            .collect();

        let outputs: Vec<_> = g
            .nodes_of(Kind::Output)
            .map(|n| {
                json!({
                    "name": n.name,
                    "label": n.label(),
                    "detail": n.detail(),
                    "icon": output_icon(n),
                    "ids": [n.id],
                    "volume": n.volume,
                    "mute": n.mute,
                    "isDefault": is_default(&n.name),
                })
            })
            .collect();

        json!({ "apps": apps, "mics": mics, "buses": buses, "outputs": outputs })
    }
}

fn output_icon(node: &NodeInfo) -> String {
    if let Some(icon) = &node.icon {
        return icon.clone();
    }
    let name = node.name.to_lowercase();
    if name.starts_with("bluez") {
        "audio-headphones".into()
    } else if name.contains("hdmi") {
        "video-display".into()
    } else {
        "audio-card".into()
    }
}

/// A virtual source that recording apps see as a microphone. Its input ports
/// are fed from the bus monitor by the reconciler.
fn create_bus_mic(core: &pw::core::CoreRc, bus: &BusConfig) -> Result<Node, pw::Error> {
    core.create_object::<Node>(
        "adapter",
        &properties! {
            "factory.name" => "support.null-audio-sink",
            "node.name" => bus.mic_node_name(),
            "node.description" => format!("{} (Hepcat mic)", bus.label),
            "media.class" => "Audio/Source/Virtual",
            "audio.position" => "FL,FR",
            "node.virtual" => "true",
            // Never preferred over a real microphone as the default input.
            "priority.session" => "1",
            "object.linger" => "true",
        },
    )
}

fn create_bus(core: &pw::core::CoreRc, bus: &BusConfig) -> Result<Node, pw::Error> {
    core.create_object::<Node>(
        "adapter",
        &properties! {
            "factory.name" => "support.null-audio-sink",
            "node.name" => bus.node_name(),
            "node.description" => format!("{} (Hepcat)", bus.label),
            "media.class" => "Audio/Sink",
            "audio.position" => "FL,FR",
            "node.virtual" => "true",
            // Make the bus fader affect what the bus plays to its output.
            "monitor.channel-volumes" => "true",
            // Buses outlive the Hepcat window; they are reused on next start.
            "object.linger" => "true",
        },
    )
}
