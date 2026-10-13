//! Persistent user configuration: which buses exist, where each bus is sent,
//! and which sources are routed into which buses.
//!
//! Routes are keyed by stable identities (application name, device node.name)
//! rather than PipeWire object ids, so they survive app restarts, replugging
//! devices and reboots.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Prefix for the node.name of every virtual bus we create.
pub const BUS_PREFIX: &str = "hepcat.bus.";
/// Prefix for the virtual microphones that expose a bus to recording apps.
pub const MIC_PREFIX: &str = "hepcat.mic.";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BusConfig {
    /// Short identifier, used in node.name (`hepcat.bus.<name>`) and in routes.
    pub name: String,
    /// Human readable label shown in the UI and as the sink description.
    pub label: String,
    /// node.name of the hardware sink this bus plays to, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Last known friendly name of `output`, shown while it is disconnected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_label: Option<String>,
    /// Also offer this bus as a microphone, e.g. for Discord or OBS.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mic: bool,
}

impl BusConfig {
    pub fn node_name(&self) -> String {
        format!("{BUS_PREFIX}{}", self.name)
    }

    pub fn mic_node_name(&self) -> String {
        format!("{MIC_PREFIX}{}", self.name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Config {
    #[serde(default, rename = "bus")]
    pub buses: Vec<BusConfig>,
    /// Source key (`app:<name>` or `mic:<node.name>`) -> bus names.
    /// The first bus of an app route is its primary target.
    #[serde(default)]
    pub routes: BTreeMap<String, Vec<String>>,
}

impl Config {
    fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("hepcat").join("config.toml"))
    }

    /// Load the config, or `None` if this is the first run.
    pub fn load() -> Option<Self> {
        let path = Self::path()?;
        Self::migrate_from_mixwire(&path);
        let text = std::fs::read_to_string(&path).ok()?;
        match toml::from_str(&text) {
            Ok(cfg) => Some(cfg),
            Err(e) => {
                eprintln!("hepcat: ignoring unreadable config {}: {e}", path.display());
                None
            }
        }
    }

    /// Earlier builds were called Mixwire and kept their config elsewhere.
    fn migrate_from_mixwire(path: &std::path::Path) {
        let Some(old) = dirs::config_dir().map(|d| d.join("mixwire").join("config.toml")) else { return };
        if path.exists() || !old.exists() {
            return;
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if std::fs::rename(&old, path).is_ok() {
            if let Some(old_dir) = old.parent() {
                let _ = std::fs::remove_dir(old_dir);
            }
        }
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match toml::to_string_pretty(self) {
            Ok(text) => {
                if let Err(e) = std::fs::write(&path, text) {
                    eprintln!("hepcat: failed to save config: {e}");
                }
            }
            Err(e) => eprintln!("hepcat: failed to serialize config: {e}"),
        }
    }

    /// Default layout for a first run: two buses, both playing to `output`.
    pub fn first_run(output: Option<String>) -> Self {
        Self {
            buses: vec![
                BusConfig { name: "main".into(), label: "Main".into(), output: output.clone(), output_label: None, mic: false },
                BusConfig { name: "chat".into(), label: "Chat".into(), output, output_label: None, mic: false },
            ],
            routes: BTreeMap::new(),
        }
    }

    pub fn bus(&self, name: &str) -> Option<&BusConfig> {
        self.buses.iter().find(|b| b.name == name)
    }

    /// Turn a user-facing label into a unique bus identifier.
    pub fn unique_bus_name(&self, label: &str) -> String {
        let mut base: String = label
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_string();
        if base.is_empty() {
            base = "bus".into();
        }
        let mut name = base.clone();
        let mut n = 2;
        while self.bus(&name).is_some() {
            name = format!("{base}-{n}");
            n += 1;
        }
        name
    }
}
