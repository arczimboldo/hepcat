//! A plain-data mirror of the parts of the PipeWire graph Hepcat cares about,
//! plus the reconciler that compares it against the user's configuration and
//! decides which links to create or remove.
//!
//! Nothing in here talks to PipeWire, which keeps the routing rules testable.

use super::meter::METER_PREFIX;
use crate::config::{BUS_PREFIX, Config, MIC_PREFIX};

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
    /// Volume on the cubic scale used by wpctl and desktop volume sliders.
    pub volume: f32,
    pub mute: bool,
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
    /// Stream node id -> `target.object` from the default metadata (a serial or node.name).
    pub stream_targets: HashMap<u32, String>,
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
    /// output ports for everything else.
    fn source_ports(&self, node: &NodeInfo) -> Vec<&PortInfo> {
        let want_monitor = matches!(node.kind(), Some(Kind::Bus | Kind::Output));
        self.ports
            .values()
            .filter(|p| p.node == node.id && p.output && p.monitor == want_monitor)
            .collect()
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
                    let Some(route) = node.route_key().and_then(|k| config.routes.get(&k)) else {
                        continue;
                    };
                    let wanted: Vec<u32> =
                        route.iter().filter_map(|b| bus_ids.get(b.as_str()).copied()).collect();
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
                        self.want_links(node, &self.nodes[&bus], &mut plan);
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
                    for link in self.links.values() {
                        let to = link.in_node;
                        let stale = !wanted.contains(&to) || Some(to) == handover;
                        if link.out_node == node.id && is_bus(to) && stale && Some(to) != keep {
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
                        self.want_links(bus, node, &mut plan);
                    }
                }
                Some(Kind::Meter) => {
                    if let Some(target) = node.metered_node().and_then(|id| self.nodes.get(&id)) {
                        self.want_links(target, node, &mut plan);
                    }
                }
                Some(Kind::Bus) => {
                    let output = node
                        .bus_name()
                        .and_then(|b| config.bus(b))
                        .and_then(|b| b.output.as_deref())
                        .and_then(|name| self.node_by_name(name))
                        .filter(|n| n.kind() == Some(Kind::Output));
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
        plan
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

    #[test]
    fn bus_links_to_other_outputs_are_removed() {
        let mut g = graph();
        g.links.insert(98, LinkInfo { id: 98, out_node: 5, out_port: 52, in_node: 3, in_port: 30 });
        let plan = g.plan(&config());
        assert!(plan.destroy.contains(&98));
    }
}
