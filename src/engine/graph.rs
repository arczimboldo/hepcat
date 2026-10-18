//! A plain-data mirror of the parts of the PipeWire graph Hepcat cares about,
//! plus the reconciler that compares it against the user's configuration and
//! decides which links to create or remove.
//!
//! Nothing in here talks to PipeWire, which keeps the routing rules testable.

use super::fx::{FX_IN_PREFIX, FX_OUT_PREFIX, FX_PREFIX, FX_REF_PREFIX, FxNames};
use super::meter::METER_PREFIX;
use crate::config::{BUS_PREFIX, Config, FxTarget, MIC_PREFIX};

/// Node names used by earlier builds, when the app was called Mixwire.
pub const LEGACY_PREFIX: &str = "mixwire.";
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// An application playing audio (Stream/Output/Audio).
    App,
    /// A capture device such as a microphone (Audio/Source).
    Mic,
    /// A hardware (or third-party virtual) playback device (Audio/Sink).
    Output,
    /// One of our own virtual buses.
    Bus,
    /// One of our level-meter capture streams.
    Meter,
    /// A virtual microphone fed by one of our buses.
    BusMic,
    /// The virtual device of a mic or app with effects: the processed
    /// microphone, or the output the app plays into ("… (Hepcat FX)").
    FxDevice,
    /// Our stream that captures the audio to process.
    FxIn,
    /// Our stream that captures the speakers for echo cancellation.
    FxRef,
    /// Our stream that plays the processed audio.
    FxOut,
}

#[derive(Debug, Clone, Default)]
pub struct NodeInfo {
    pub id: u32,
    pub serial: String,
    pub name: String,
    pub description: String,
    pub media_class: String,
    pub app_name: Option<String>,
    pub binary: Option<String>,
    pub icon: Option<String>,
    pub media_name: Option<String>,
    /// Volume on the cubic scale used by desktop volume sliders.
    pub volume: f32,
    pub mute: bool,
    /// Number of channels in the node's volume.
    pub channels: usize,
    /// For hardware nodes: the device and its `card.profile.device`, which
    /// address the route that carries the real volume.
    pub device: Option<(u32, i32)>,
}

impl NodeInfo {
    pub fn kind(&self) -> Option<Kind> {
        if self.name.starts_with(LEGACY_PREFIX) {
            return None;
        }
        if self.name.starts_with(METER_PREFIX) {
            return Some(Kind::Meter);
        }
        if self.name.starts_with(MIC_PREFIX) {
            return Some(Kind::BusMic);
        }
        if self.name.starts_with(FX_PREFIX) {
            return Some(Kind::FxDevice);
        }
        if self.name.starts_with(FX_IN_PREFIX) {
            return Some(Kind::FxIn);
        }
        if self.name.starts_with(FX_REF_PREFIX) {
            return Some(Kind::FxRef);
        }
        if self.name.starts_with(FX_OUT_PREFIX) {
            return Some(Kind::FxOut);
        }
        match self.media_class.as_str() {
            "Stream/Output/Audio" if !self.name.starts_with("hepcat") => Some(Kind::App),
            "Audio/Source" | "Audio/Source/Virtual" => Some(Kind::Mic),
            "Audio/Sink" if self.name.starts_with(BUS_PREFIX) => Some(Kind::Bus),
            "Audio/Sink" => Some(Kind::Output),
            _ => None,
        }
    }

    pub fn label(&self) -> String {
        if self.kind() == Some(Kind::App) {
            if let Some(name) = self.app_name.as_ref().or(self.binary.as_ref()) {
                return name.clone();
            }
        }
        if self.kind() == Some(Kind::Output) {
            // "Navi 21/23 HDMI/DP Audio Controller Digital Stereo (HDMI) [MSI MP271A E2]":
            // the bracketed monitor name is what people recognise.
            if let Some((_, monitor)) = self.split_description() {
                return monitor.to_string();
            }
        }
        if !self.description.is_empty() {
            return self.description.clone();
        }
        self.name.clone()
    }

    /// Secondary line shown under the label.
    pub fn detail(&self) -> String {
        match self.kind() {
            Some(Kind::App) => self.media_name.clone().unwrap_or_default(),
            Some(Kind::Output) => self
                .split_description()
                .map(|(device, _)| device.to_string())
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    fn split_description(&self) -> Option<(&str, &str)> {
        let d = self.description.strip_suffix(']')?;
        let (device, monitor) = d.rsplit_once(" [")?;
        Some((device, monitor))
    }

    /// Stable identity used as the key for routing rules.
    pub fn route_key(&self) -> Option<String> {
        match self.kind()? {
            Kind::App => Some(format!(
                "app:{}",
                self.app_name.as_ref().or(self.binary.as_ref()).unwrap_or(&self.name)
            )),
            Kind::Mic => Some(format!("mic:{}", self.name)),
            _ => None,
        }
    }

    /// For a meter stream, the id of the node it measures.
    pub fn metered_node(&self) -> Option<u32> {
        self.name.strip_prefix(METER_PREFIX)?.parse().ok()
    }

    pub fn bus_name(&self) -> Option<&str> {
        self.name.strip_prefix(BUS_PREFIX)
    }
}

#[derive(Debug, Clone)]
pub struct PortInfo {
    pub id: u32,
    pub node: u32,
    pub output: bool,
    pub monitor: bool,
    pub channel: String,
}

#[derive(Debug, Clone)]
pub struct LinkInfo {
    pub id: u32,
    pub out_node: u32,
    pub out_port: u32,
    pub in_node: u32,
    pub in_port: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PortPair {
    pub out_node: u32,
    pub out_port: u32,
    pub in_node: u32,
    pub in_port: u32,
}

#[derive(Debug, Default)]
pub struct Graph {
    pub nodes: BTreeMap<u32, NodeInfo>,
    pub ports: BTreeMap<u32, PortInfo>,
    pub links: BTreeMap<u32, LinkInfo>,
    /// node.name of the current default sink.
    pub default_sink: Option<String>,
    /// node.name of the current default source.
    pub default_source: Option<String>,
    /// Stream node id -> `target.object` from the default metadata (a serial or node.name).
    pub stream_targets: HashMap<u32, String>,
}

/// The nodes making up one strip's effects; any may still be missing while
/// they start up.
#[derive(Debug, Clone, Copy)]
pub struct FxNodes<'a> {
    pub device: Option<&'a NodeInfo>,
    pub input: Option<&'a NodeInfo>,
    pub output: Option<&'a NodeInfo>,
    pub reference: Option<&'a NodeInfo>,
}

/// What the reconciler wants changed in the graph.
#[derive(Debug, Default, PartialEq)]
pub struct Plan {
    pub create: Vec<PortPair>,
    pub destroy: Vec<u32>,
    /// Stream node id -> object.serial of the bus WirePlumber should send it to.
    pub targets: BTreeMap<u32, String>,
}

impl Graph {
    pub fn node_by_name(&self, name: &str) -> Option<&NodeInfo> {
        self.nodes.values().find(|n| n.name == name)
    }

    /// The node WirePlumber is keeping `stream` on: its explicit target, or
    /// else the default sink. Links to it belong to WirePlumber; removing them
    /// behind its back leaves the stream unlinked for good.
    pub fn policy_target(&self, stream: u32) -> Option<u32> {
        match self.stream_targets.get(&stream) {
            Some(t) => self.nodes.values().find(|n| &n.serial == t || &n.name == t).map(|n| n.id),
            None => self.default_sink.as_deref().and_then(|d| self.node_by_name(d)).map(|n| n.id),
        }
    }

    /// The running nodes of the effects of strip `key`, if they are on.
    pub fn fx(&self, config: &Config, key: &str) -> Option<FxNodes<'_>> {
        config.active_fx(key)?;
        let names = FxNames::new(key);
        Some(FxNodes {
            device: self.node_by_name(&names.device),
            input: self.node_by_name(&names.input),
            output: self.node_by_name(&names.output),
            reference: self.node_by_name(&names.reference),
        })
    }

    /// The hardware device a bus plays to.
    fn bus_output(&self, config: &Config, bus: &NodeInfo) -> Option<&NodeInfo> {
        bus.bus_name()
            .and_then(|b| config.bus(b))
            .and_then(|b| b.output.as_deref())
            .and_then(|name| self.node_by_name(name))
            .filter(|n| n.kind() == Some(Kind::Output))
    }

    /// The device whose sound echo cancellation should remove: the hardware
    /// output new apps play to. When that is a bus, its own output, because
    /// that also carries every other bus playing to the same speakers.
    pub fn echo_reference(&self, config: &Config) -> Option<&NodeInfo> {
        let default = self.node_by_name(self.default_sink.as_deref()?)?;
        if default.kind() == Some(Kind::Bus) {
            return Some(self.bus_output(config, default).unwrap_or(default));
        }
        Some(default)
    }

    pub fn nodes_of(&self, kind: Kind) -> impl Iterator<Item = &NodeInfo> {
        self.nodes.values().filter(move |n| n.kind() == Some(kind))
    }

    /// Nodes that `node` currently sends audio into.
    pub fn linked_targets(&self, node: u32) -> BTreeSet<u32> {
        self.links.values().filter(|l| l.out_node == node).map(|l| l.in_node).collect()
    }

    fn has_link(&self, out_port: u32, in_port: u32) -> bool {
        self.links.values().any(|l| l.out_port == out_port && l.in_port == in_port)
    }

    /// Ports that carry a node's audio *out*: monitor ports for sinks, regular
    /// output ports for everything else. Virtual sources made from a null
    /// sink only have monitor ports, so those are used when nothing else is there.
    fn source_ports(&self, node: &NodeInfo) -> Vec<&PortInfo> {
        let outputs = || self.ports.values().filter(|p| p.node == node.id && p.output);
        if matches!(node.kind(), Some(Kind::Bus | Kind::Output)) {
            return outputs().filter(|p| p.monitor).collect();
        }
        let regular: Vec<_> = outputs().filter(|p| !p.monitor).collect();
        if regular.is_empty() { outputs().collect() } else { regular }
    }

    fn sink_ports(&self, node: &NodeInfo) -> Vec<&PortInfo> {
        self.ports.values().filter(|p| p.node == node.id && !p.output).collect()
    }

    /// Port pairs needed to send all of `from`'s audio into `to`.
    pub fn port_pairs(&self, from: &NodeInfo, to: &NodeInfo) -> Vec<PortPair> {
        let outs = self.source_ports(from);
        let ins = self.sink_ports(to);
        map_channels(&outs, &ins)
            .into_iter()
            .map(|(o, i)| PortPair { out_node: from.id, out_port: o, in_node: to.id, in_port: i })
            .collect()
    }

    /// Compare the graph with the config and work out what to change.
    pub fn plan(&self, config: &Config) -> Plan {
        let mut plan = Plan::default();
        let bus_ids: HashMap<&str, u32> = self
            .nodes_of(Kind::Bus)
            .filter_map(|n| Some((n.bus_name()?, n.id)))
            .collect();
        let is_bus = |id: u32| bus_ids.values().any(|&b| b == id);

        for node in self.nodes.values() {
            match node.kind() {
                Some(kind @ (Kind::App | Kind::Mic)) => {
                    let key = node.route_key().unwrap_or_default();
                    let fx_device = self.fx(config, &key).and_then(|f| f.device);
                    if kind == Kind::App {
                        if let Some(device) = fx_device {
                            // The app plays into its effects; those feed its buses.
                            plan.targets.insert(node.id, device.serial.clone());
                            let keep = self.policy_target(node.id);
                            for link in self.links.values() {
                                if link.out_node == node.id && is_bus(link.in_node) && Some(link.in_node) != keep {
                                    plan.destroy.push(link.id);
                                }
                            }
                            continue;
                        }
                    }
                    let Some(route) = config.routes.get(&key) else {
                        continue;
                    };
                    let wanted: Vec<u32> =
                        route.iter().filter_map(|b| bus_ids.get(b.as_str()).copied()).collect();
                    // A mic with effects feeds its buses through its processed copy.
                    let fx = fx_device.filter(|_| kind == Kind::Mic);
                    let from = fx.unwrap_or(node);
                    // Apps reach their first bus through WirePlumber (so it also
                    // survives our process exiting); the rest are linked by us.
                    let manual = match kind {
                        Kind::App => {
                            if let Some(&first) = wanted.first() {
                                plan.targets.insert(node.id, self.nodes[&first].serial.clone());
                            }
                            wanted.get(1..).unwrap_or_default()
                        }
                        _ => &wanted[..],
                    };
                    for &bus in manual {
                        self.want_links(from, &self.nodes[&bus], &mut plan);
                    }
                    // Apps leave a bus WirePlumber is handling by retargeting,
                    // which makes WirePlumber drop its own links. When a bus we
                    // linked by hand becomes the primary, our links go first so
                    // WirePlumber can take it over cleanly.
                    let keep = if kind == Kind::App { self.policy_target(node.id) } else { None };
                    let handover = match kind {
                        Kind::App => wanted.first().copied().filter(|&p| Some(p) != keep),
                        _ => None,
                    };
                    let fx_id = fx.map(|f| f.id);
                    for link in self.links.values() {
                        let to = link.in_node;
                        let ours = link.out_node == node.id || Some(link.out_node) == fx_id;
                        if !ours || !is_bus(to) || Some(to) == keep {
                            continue;
                        }
                        if link.out_node != from.id || !wanted.contains(&to) || Some(to) == handover {
                            plan.destroy.push(link.id);
                        }
                    }
                }
                Some(Kind::BusMic) => {
                    let bus = node
                        .name
                        .strip_prefix(MIC_PREFIX)
                        .and_then(|b| self.node_by_name(&format!("{BUS_PREFIX}{b}")));
                    if let Some(bus) = bus {
                        // With effects on, the bus mic carries the processed
                        // sound (linked in `plan_fx`).
                        let key = format!("bus:{}", bus.bus_name().unwrap_or_default());
                        let from = self.fx(config, &key).and_then(|f| f.output).unwrap_or(bus);
                        if from.id == bus.id {
                            self.want_links(bus, node, &mut plan);
                        }
                        self.unlink_into(node, &[from.id], &mut plan);
                    }
                }
                Some(Kind::Meter) => {
                    if let Some(target) = node.metered_node().and_then(|id| self.nodes.get(&id)) {
                        self.want_links(target, node, &mut plan);
                    }
                }
                Some(Kind::Bus) => {
                    // With effects running, the processed copy goes to the device instead.
                    let key = format!("bus:{}", node.bus_name().unwrap_or_default());
                    let processed = self.fx(config, &key).and_then(|f| f.output).is_some();
                    let output = self.bus_output(config, node).filter(|_| !processed);
                    if let Some(output) = output {
                        self.want_links(node, output, &mut plan);
                    }
                    for link in self.links.values() {
                        if link.out_node != node.id || Some(link.in_node) == output.map(|o| o.id) {
                            continue;
                        }
                        let target_kind = self.nodes.get(&link.in_node).and_then(NodeInfo::kind);
                        if matches!(target_kind, Some(Kind::Output | Kind::Bus)) {
                            plan.destroy.push(link.id);
                        }
                    }
                }
                _ => {}
            }
        }
        for key in config.active_fx_keys() {
            self.plan_fx(config, key, &bus_ids, &mut plan);
        }
        plan
    }

    /// Feed one strip's effects and send their result where it belongs.
    fn plan_fx(&self, config: &Config, key: &str, bus_ids: &HashMap<&str, u32>, plan: &mut Plan) {
        let (Some(target), Some(fx)) = (FxTarget::parse(key), self.fx(config, key)) else { return };
        let bus = |name: &str| self.node_by_name(&format!("{BUS_PREFIX}{name}"));

        let source = match target {
            FxTarget::Mic(name) => self.node_by_name(name).filter(|n| n.kind() == Some(Kind::Mic)),
            FxTarget::App(_) => fx.device,
            FxTarget::Bus(name) => bus(name),
        };
        if let Some(input) = fx.input {
            if let Some(source) = source {
                self.want_links(source, input, plan);
            }
            self.unlink_into(input, &source.map(|s| s.id).into_iter().collect::<Vec<_>>(), plan);
        }

        let destinations: Vec<&NodeInfo> = match target {
            FxTarget::Mic(_) => fx.device.into_iter().collect(),
            FxTarget::App(_) => {
                let route: Vec<&NodeInfo> = config
                    .routes
                    .get(key)
                    .into_iter()
                    .flatten()
                    .filter_map(|b| bus_ids.get(b.as_str()))
                    .filter_map(|id| self.nodes.get(id))
                    .collect();
                if route.is_empty() {
                    // Like any app without a route: the default output.
                    self.default_sink
                        .as_deref()
                        .and_then(|d| self.node_by_name(d))
                        .filter(|n| matches!(n.kind(), Some(Kind::Output | Kind::Bus)))
                        .into_iter()
                        .collect()
                } else {
                    route
                }
            }
            // A bus plays to its device and its microphone.
            FxTarget::Bus(name) => bus(name)
                .and_then(|b| self.bus_output(config, b))
                .into_iter()
                .chain(self.node_by_name(&format!("{MIC_PREFIX}{name}")))
                .collect(),
        };
        if let Some(output) = fx.output {
            for to in &destinations {
                self.want_links(output, to, plan);
            }
            for link in self.links.values() {
                if link.out_node == output.id && !destinations.iter().any(|d| d.id == link.in_node) {
                    plan.destroy.push(link.id);
                }
            }
        }

        if let Some(reference) = fx.reference {
            // The echo is whatever plays on the speakers, except this strip's
            // own sound: when it is heard there too (a mic or a phone sent to
            // a bus you listen to), cancelling it would silence it.
            let mut keep = vec![];
            if let Some(device) = self.echo_reference(config) {
                let heard = self.sources_of(device);
                let own = fx.input.map(|input| self.sources_of(input)).unwrap_or_default();
                for id in heard.difference(&own) {
                    self.want_links(&self.nodes[id], reference, plan);
                    keep.push(*id);
                }
            }
            // Follow the default output and what plays on it.
            self.unlink_into(reference, &keep, plan);
        }
    }

    /// The streams and inputs whose sound ends up in `node`, followed back
    /// through buses, virtual devices and effects.
    pub fn sources_of(&self, node: &NodeInfo) -> BTreeSet<u32> {
        let mut seen = BTreeSet::from([node.id]);
        let mut sources = BTreeSet::new();
        let mut queue = vec![node.id];
        while let Some(id) = queue.pop() {
            for link in self.links.values().filter(|l| l.in_node == id) {
                let Some(from) = self.nodes.get(&link.out_node) else { continue };
                if !seen.insert(from.id) {
                    continue;
                }
                match from.kind() {
                    Some(Kind::Bus | Kind::Output | Kind::FxDevice) => queue.push(from.id),
                    // Effects output: what went into the effects.
                    Some(Kind::FxOut) => {
                        let input = from.name.strip_prefix(FX_OUT_PREFIX).map(|slug| format!("{FX_IN_PREFIX}{slug}"));
                        if let Some(input) = input.and_then(|name| self.node_by_name(&name)) {
                            queue.push(input.id);
                        }
                    }
                    Some(Kind::Meter | Kind::FxIn | Kind::FxRef | Kind::BusMic) => {}
                    _ => {
                        sources.insert(from.id);
                    }
                }
            }
        }
        sources
    }

    /// Remove links into `node` from anything but `keep`.
    fn unlink_into(&self, node: &NodeInfo, keep: &[u32], plan: &mut Plan) {
        for link in self.links.values() {
            if link.in_node == node.id && !keep.contains(&link.out_node) {
                plan.destroy.push(link.id);
            }
        }
    }

    fn want_links(&self, from: &NodeInfo, to: &NodeInfo, plan: &mut Plan) {
        for pair in self.port_pairs(from, to) {
            if !self.has_link(pair.out_port, pair.in_port) {
                plan.create.push(pair);
            }
        }
    }
}

/// Match output ports to input ports by channel position. Mono sources fan out
/// to every input; anything into a mono input is mixed down; channel layouts
/// with no names in common are paired in order.
fn map_channels(outs: &[&PortInfo], ins: &[&PortInfo]) -> Vec<(u32, u32)> {
    if outs.is_empty() || ins.is_empty() {
        return vec![];
    }
    if outs.len() == 1 {
        return ins.iter().map(|i| (outs[0].id, i.id)).collect();
    }
    if ins.len() == 1 {
        return outs.iter().map(|o| (o.id, ins[0].id)).collect();
    }
    let by_channel: Vec<(u32, u32)> = ins
        .iter()
        .filter_map(|i| outs.iter().find(|o| o.channel == i.channel).map(|o| (o.id, i.id)))
        .collect();
    if !by_channel.is_empty() {
        return by_channel;
    }
    outs.iter().zip(ins.iter()).map(|(o, i)| (o.id, i.id)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BusConfig;

    fn node(id: u32, name: &str, class: &str) -> NodeInfo {
        NodeInfo {
            id,
            serial: format!("{}", id + 1000),
            name: name.into(),
            media_class: class.into(),
            ..Default::default()
        }
    }

    fn stereo(g: &mut Graph, node: u32, first_port: u32, output: bool, monitor: bool) {
        for (i, ch) in ["FL", "FR"].iter().enumerate() {
            let id = first_port + i as u32;
            g.ports.insert(id, PortInfo { id, node, output, monitor, channel: ch.to_string() });
        }
    }

    /// Spotify (stream), a mono mic, headphones, and two buses.
    fn graph() -> Graph {
        let mut g = Graph::default();
        let mut spotify = node(1, "spotify", "Stream/Output/Audio");
        spotify.app_name = Some("Spotify".into());
        g.nodes.insert(1, spotify);
        g.nodes.insert(2, node(2, "alsa_input.mic", "Audio/Source"));
        g.nodes.insert(3, node(3, "headphones", "Audio/Sink"));
        g.nodes.insert(4, node(4, "hepcat.bus.main", "Audio/Sink"));
        g.nodes.insert(5, node(5, "hepcat.bus.chat", "Audio/Sink"));
        stereo(&mut g, 1, 10, true, false);
        g.ports.insert(20, PortInfo { id: 20, node: 2, output: true, monitor: false, channel: "MONO".into() });
        stereo(&mut g, 3, 30, false, false);
        stereo(&mut g, 3, 32, true, true);
        for bus in [4, 5] {
            stereo(&mut g, bus, bus * 10, false, false);
            stereo(&mut g, bus, bus * 10 + 2, true, true);
        }
        g
    }

    fn config() -> Config {
        Config {
            buses: vec![
                BusConfig {
                    name: "main".into(),
                    label: "Main".into(),
                    output: Some("headphones".into()),
                    output_label: None,
                    mic: false,
                },
                BusConfig { name: "chat".into(), label: "Chat".into(), output: None, output_label: None, mic: true },
            ],
            routes: Default::default(),
            fx: Default::default(),
            apps: Default::default(),
        }
    }

    #[test]
    fn bus_is_linked_to_its_output_by_channel() {
        let plan = graph().plan(&config());
        assert_eq!(
            plan.create,
            vec![
                PortPair { out_node: 4, out_port: 42, in_node: 3, in_port: 30 },
                PortPair { out_node: 4, out_port: 43, in_node: 3, in_port: 31 },
            ]
        );
        assert!(plan.targets.is_empty());
    }

    #[test]
    fn app_primary_bus_is_a_target_and_others_are_links() {
        let mut cfg = config();
        cfg.routes.insert("app:Spotify".into(), vec!["chat".into(), "main".into()]);
        let plan = graph().plan(&cfg);
        assert_eq!(plan.targets.get(&1), Some(&"1005".to_string()));
        assert!(plan.create.contains(&PortPair { out_node: 1, out_port: 10, in_node: 4, in_port: 40 }));
        assert!(!plan.create.iter().any(|p| p.out_node == 1 && p.in_node == 5));
    }

    #[test]
    fn mono_mic_fans_out_and_stale_links_are_removed() {
        let mut g = graph();
        g.links.insert(99, LinkInfo { id: 99, out_node: 2, out_port: 20, in_node: 4, in_port: 40 });
        let mut cfg = config();
        cfg.routes.insert("mic:alsa_input.mic".into(), vec!["chat".into()]);
        let plan = g.plan(&cfg);
        assert!(plan.destroy.contains(&99));
        let mic: Vec<_> = plan.create.iter().filter(|p| p.out_node == 2).collect();
        assert_eq!(mic.len(), 2);
        assert!(mic.iter().all(|p| p.out_port == 20 && p.in_node == 5));
    }

    #[test]
    fn links_owned_by_wireplumber_are_never_removed() {
        let mut g = graph();
        // Spotify sits on the main bus via WirePlumber and on chat via us.
        g.stream_targets.insert(1, "1004".into());
        g.links.insert(97, LinkInfo { id: 97, out_node: 1, out_port: 10, in_node: 4, in_port: 40 });
        g.links.insert(96, LinkInfo { id: 96, out_node: 1, out_port: 10, in_node: 5, in_port: 50 });
        let mut cfg = config();
        cfg.routes.insert("app:Spotify".into(), vec![]);
        let plan = g.plan(&cfg);
        assert_eq!(plan.destroy, vec![96]);

        // Same when there is no explicit target but main is the default sink.
        g.stream_targets.clear();
        g.default_sink = Some("hepcat.bus.main".into());
        assert_eq!(g.plan(&cfg).destroy, vec![96]);
    }

    #[test]
    fn manual_links_are_dropped_when_their_bus_becomes_primary() {
        let mut g = graph();
        // WirePlumber has Spotify on chat; we added main by hand.
        g.stream_targets.insert(1, "1005".into());
        g.links.insert(97, LinkInfo { id: 97, out_node: 1, out_port: 10, in_node: 5, in_port: 50 });
        g.links.insert(96, LinkInfo { id: 96, out_node: 1, out_port: 10, in_node: 4, in_port: 40 });
        let mut cfg = config();
        cfg.routes.insert("app:Spotify".into(), vec!["main".into()]);
        let plan = g.plan(&cfg);
        assert_eq!(plan.destroy, vec![96]);
        assert_eq!(plan.targets.get(&1), Some(&"1004".to_string()));
    }

    #[test]
    fn meters_are_fed_from_monitor_or_output_ports() {
        let mut g = graph();
        g.nodes.insert(6, node(6, "hepcat.meter.4", "Stream/Input/Audio/Meter"));
        g.nodes.insert(7, node(7, "hepcat.meter.1", "Stream/Input/Audio/Meter"));
        stereo(&mut g, 6, 60, false, false);
        stereo(&mut g, 7, 70, false, false);
        let plan = g.plan(&config());
        let into = |n: u32| plan.create.iter().filter(|p| p.in_node == n).map(|p| (p.out_port, p.in_port)).collect::<Vec<_>>();
        assert_eq!(into(6), vec![(42, 60), (43, 61)]); // bus monitor ports
        assert_eq!(into(7), vec![(10, 70), (11, 71)]); // app output ports
    }

    #[test]
    fn bus_mic_is_fed_from_the_bus_monitor() {
        let mut g = graph();
        g.nodes.insert(8, node(8, "hepcat.mic.chat", "Audio/Source/Virtual"));
        stereo(&mut g, 8, 80, false, false);
        stereo(&mut g, 8, 82, true, true);
        let plan = g.plan(&config());
        let into: Vec<_> = plan.create.iter().filter(|p| p.in_node == 8).map(|p| (p.out_port, p.in_port)).collect();
        assert_eq!(into, vec![(52, 80), (53, 81)]);
        assert!(g.nodes[&8].kind() == Some(Kind::BusMic));
    }

    /// Add the nodes of running effects for `key`: device (if any), input and
    /// output, with ids from `first`.
    fn add_fx(g: &mut Graph, key: &str, first: u32, device_class: Option<&str>, out_channels: &[&str]) {
        let names = FxNames::new(key);
        if let Some(class) = device_class {
            g.nodes.insert(first, node(first, &names.device, class));
            stereo(g, first, first * 10, false, false);
            stereo(g, first, first * 10 + 2, true, true);
        }
        g.nodes.insert(first + 1, node(first + 1, &names.input, "Stream/Input/Audio/Effects"));
        stereo(g, first + 1, (first + 1) * 10, false, false);
        g.nodes.insert(first + 2, node(first + 2, &names.output, "Stream/Output/Audio/Effects"));
        for (i, ch) in out_channels.iter().enumerate() {
            let id = (first + 2) * 10 + i as u32;
            g.ports.insert(id, PortInfo { id, node: first + 2, output: true, monitor: false, channel: ch.to_string() });
        }
    }

    fn with_fx(mut cfg: Config, key: &str) -> Config {
        cfg.fx.insert(key.into(), crate::config::Effects { enabled: true, ..Default::default() });
        cfg
    }

    fn into(plan: &Plan, n: u32) -> Vec<(u32, u32)> {
        plan.create.iter().filter(|p| p.in_node == n).map(|p| (p.out_node, p.out_port)).collect()
    }

    /// Mic effects: virtual mic 8, input 9, output 10.
    fn fx_graph() -> (Graph, Config) {
        let mut g = graph();
        add_fx(&mut g, "mic:alsa_input.mic", 8, Some("Audio/Source/Virtual"), &["MONO"]);
        let mut cfg = with_fx(config(), "mic:alsa_input.mic");
        cfg.routes.insert("mic:alsa_input.mic".into(), vec!["chat".into()]);
        (g, cfg)
    }

    #[test]
    fn mic_with_effects_routes_its_processed_copy() {
        let (mut g, cfg) = fx_graph();
        // The raw mic was on chat before effects were switched on.
        g.links.insert(99, LinkInfo { id: 99, out_node: 2, out_port: 20, in_node: 5, in_port: 50 });
        let plan = g.plan(&cfg);
        assert!(plan.destroy.contains(&99));
        assert_eq!(into(&plan, 5), vec![(8, 82), (8, 83)]); // virtual mic to the bus, from its monitor ports
        assert_eq!(into(&plan, 9), vec![(2, 20), (2, 20)]); // raw mic feeds the effects
        assert_eq!(into(&plan, 8), vec![(10, 100), (10, 100)]); // processed audio into the virtual mic
        assert!(g.nodes[&8].kind() == Some(Kind::FxDevice));
    }

    #[test]
    fn switching_effects_off_moves_the_route_back_to_the_raw_mic() {
        let (mut g, mut cfg) = fx_graph();
        g.links.insert(98, LinkInfo { id: 98, out_node: 8, out_port: 82, in_node: 5, in_port: 50 });
        cfg.fx.get_mut("mic:alsa_input.mic").unwrap().enabled = false;
        let plan = g.plan(&cfg);
        // The virtual mic is no longer the route, but it is not a mic either,
        // so its old links stay until the node itself goes away.
        assert!(plan.create.iter().any(|p| p.out_node == 2 && p.in_node == 5));
    }

    #[test]
    fn app_with_effects_plays_into_them_and_they_feed_its_buses() {
        let mut g = graph();
        add_fx(&mut g, "app:Spotify", 11, Some("Audio/Sink"), &["FL", "FR"]);
        // Spotify sits on chat via WirePlumber and on main via us.
        g.stream_targets.insert(1, "1005".into());
        g.links.insert(97, LinkInfo { id: 97, out_node: 1, out_port: 10, in_node: 5, in_port: 50 });
        g.links.insert(96, LinkInfo { id: 96, out_node: 1, out_port: 10, in_node: 4, in_port: 40 });
        let mut cfg = with_fx(config(), "app:Spotify");
        cfg.routes.insert("app:Spotify".into(), vec!["chat".into(), "main".into()]);
        let plan = g.plan(&cfg);
        assert_eq!(plan.targets.get(&1), Some(&"1011".to_string()));
        assert_eq!(plan.destroy, vec![96]); // ours goes; WirePlumber moves its own
        assert_eq!(into(&plan, 12), vec![(11, 112), (11, 113)]); // virtual output's monitor into effects
        assert_eq!(into(&plan, 5), vec![(13, 130), (13, 131)]);
        assert_eq!(into(&plan, 4), vec![(13, 130), (13, 131)]);
        assert!(g.nodes[&11].kind() == Some(Kind::FxDevice)); // not listed as an output
    }

    #[test]
    fn app_with_effects_and_no_route_goes_to_the_default_output() {
        let mut g = graph();
        add_fx(&mut g, "app:Spotify", 11, Some("Audio/Sink"), &["FL", "FR"]);
        g.default_sink = Some("headphones".into());
        let plan = g.plan(&with_fx(config(), "app:Spotify"));
        let from_fx: Vec<_> = into(&plan, 3).into_iter().filter(|&(n, _)| n == 13).collect();
        assert_eq!(from_fx, vec![(13, 130), (13, 131)]);
    }

    #[test]
    fn bus_with_effects_reaches_its_device_through_them() {
        let mut g = graph();
        add_fx(&mut g, "bus:main", 14, None, &["FL", "FR"]);
        g.nodes.insert(18, node(18, "hepcat.mic.main", "Audio/Source/Virtual"));
        stereo(&mut g, 18, 180, false, false);
        // Before effects: bus straight to the headphones and its mic.
        g.links.insert(95, LinkInfo { id: 95, out_node: 4, out_port: 42, in_node: 3, in_port: 30 });
        g.links.insert(94, LinkInfo { id: 94, out_node: 4, out_port: 42, in_node: 18, in_port: 180 });
        // The effects already feed the bus mic on the left channel.
        g.links.insert(93, LinkInfo { id: 93, out_node: 16, out_port: 160, in_node: 18, in_port: 180 });
        let plan = g.plan(&with_fx(config(), "bus:main"));
        assert!(plan.destroy.contains(&95) && plan.destroy.contains(&94));
        assert!(!plan.destroy.contains(&93), "the bus mic link must not be torn down");
        assert_eq!(into(&plan, 15), vec![(4, 42), (4, 43)]); // bus monitor into effects
        assert_eq!(into(&plan, 3), vec![(16, 160), (16, 161)]); // processed to the headphones
        assert_eq!(into(&plan, 18), vec![(16, 161)]); // and to the bus mic
    }

    /// Main plays to the headphones; Spotify plays into main.
    fn listening_graph() -> Graph {
        let mut g = graph();
        g.default_sink = Some("hepcat.bus.main".into());
        g.links.insert(90, LinkInfo { id: 90, out_node: 4, out_port: 42, in_node: 3, in_port: 30 });
        g.links.insert(91, LinkInfo { id: 91, out_node: 1, out_port: 10, in_node: 4, in_port: 40 });
        g.links.insert(92, LinkInfo { id: 92, out_node: 1, out_port: 11, in_node: 4, in_port: 41 });
        g
    }

    fn add_reference(g: &mut Graph, key: &str, id: u32) {
        g.nodes.insert(id, node(id, &FxNames::new(key).reference, "Stream/Input/Audio/Effects"));
        stereo(g, id, id * 10, false, false);
    }

    #[test]
    fn echo_reference_is_what_plays_on_the_speakers() {
        let mut g = listening_graph();
        let key = "mic:alsa_input.mic";
        add_reference(&mut g, key, 17);
        // An old link from the bus monitor goes.
        g.links.insert(97, LinkInfo { id: 97, out_node: 5, out_port: 52, in_node: 17, in_port: 170 });
        let plan = g.plan(&with_fx(config(), key));
        assert_eq!(into(&plan, 17), vec![(1, 10), (1, 11)]); // Spotify, found through main
        assert!(plan.destroy.contains(&97));
    }

    #[test]
    fn echo_reference_leaves_out_the_strips_own_sound() {
        // A phone app with effects plays to main next to Spotify.
        let mut g = listening_graph();
        let key = "app:Phone";
        let mut phone = node(19, "phone", "Stream/Output/Audio");
        phone.app_name = Some("Phone".into());
        g.nodes.insert(19, phone);
        stereo(&mut g, 19, 190, true, false);
        add_fx(&mut g, key, 11, Some("Audio/Sink"), &["FL", "FR"]);
        add_reference(&mut g, key, 18);
        // phone → its virtual output → effects → main
        g.links.insert(80, LinkInfo { id: 80, out_node: 19, out_port: 190, in_node: 11, in_port: 110 });
        g.links.insert(81, LinkInfo { id: 81, out_node: 11, out_port: 112, in_node: 12, in_port: 120 });
        g.links.insert(82, LinkInfo { id: 82, out_node: 13, out_port: 130, in_node: 4, in_port: 40 });
        let mut cfg = with_fx(config(), key);
        cfg.routes.insert(key.into(), vec!["main".into()]);
        let plan = g.plan(&cfg);
        assert_eq!(into(&plan, 18), vec![(1, 10), (1, 11)]); // only Spotify
        assert_eq!(g.sources_of(&g.nodes[&3]), BTreeSet::from([1, 19]));
    }

    #[test]
    fn bus_links_to_other_outputs_are_removed() {
        let mut g = graph();
        g.links.insert(98, LinkInfo { id: 98, out_node: 5, out_port: 52, in_node: 3, in_port: 30 });
        let plan = g.plan(&config());
        assert!(plan.destroy.contains(&98));
    }
}
