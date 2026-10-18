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
    /// Strip key (`mic:<node.name>`, `app:<name>` or `bus:<name>`) -> its effects.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fx: BTreeMap<String, Effects>,
    /// Every app seen playing (`app:<name>` -> how it looked), offered when
    /// adding an app by hand, and the ones kept in the mixer while silent.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub apps: BTreeMap<String, KnownApp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct KnownApp {
    pub label: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub icon: String,
    /// Shown in the mixer even while it plays nothing, e.g. a game that is
    /// only heard while it has focus.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub kept: bool,
}

/// What a key in [`Config::fx`] applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxTarget<'a> {
    /// A microphone, by node.name.
    Mic(&'a str),
    /// An application, by the name in its route key.
    App(&'a str),
    /// One of our buses, by bus name.
    Bus(&'a str),
}

impl<'a> FxTarget<'a> {
    pub fn parse(key: &'a str) -> Option<Self> {
        let (kind, name) = key.split_once(':')?;
        match kind {
            "mic" => Some(Self::Mic(name)),
            "app" => Some(Self::App(name)),
            "bus" => Some(Self::Bus(name)),
            _ => None,
        }
    }

    /// Microphones are processed in mono, everything else in stereo.
    pub fn channels(self) -> usize {
        if matches!(self, Self::Mic(_)) { 1 } else { 2 }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum NoiseMode {
    #[default]
    Off,
    /// WebRTC noise suppression: removes steady background noise (fans, hum,
    /// hiss) but lets every other sound through, claps and music included.
    Standard,
    /// RNNoise: a neural network that keeps speech and drops everything else.
    Voice,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum VoiceEffect {
    #[default]
    None,
    Robot,
    Alien,
    Radio,
    Telephone,
    Megaphone,
    Underwater,
    Echo,
    Hall,
}

/// Processing applied to a microphone. While `enabled`, Hepcat offers a
/// processed copy of the mic and routes that instead of the raw device.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Effects {
    pub enabled: bool,
    /// Manual gain before anything else, in dB.
    pub gain_db: f32,
    /// Automatic gain control: keeps speech at a steady level.
    pub auto_gain: bool,
    pub noise: NoiseMode,
    /// How hard `noise` works, 1 (light) to 4 (maximum); 3 is the default.
    pub noise_strength: u8,
    /// Remove what the speakers play (other than this strip itself) from the signal.
    pub echo: bool,
    pub gate: bool,
    /// The gate opens above this level, in dBFS.
    pub gate_db: f32,
    pub bass_db: f32,
    pub presence_db: f32,
    pub treble_db: f32,
    /// Compressor amount, 0 (off) to 1.
    pub compressor: f32,
    /// Pitch shift in semitones.
    pub pitch: f32,
    pub voice: VoiceEffect,
    /// Wet/dry mix of `voice`, 0 to 1.
    pub voice_mix: f32,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            enabled: false,
            gain_db: 0.0,
            auto_gain: false,
            noise: NoiseMode::Off,
            noise_strength: 3,
            echo: false,
            gate: false,
            gate_db: -50.0,
            bass_db: 0.0,
            presence_db: 0.0,
            treble_db: 0.0,
            compressor: 0.0,
            pitch: 0.0,
            voice: VoiceEffect::None,
            voice_mix: 1.0,
        }
    }
}

impl Effects {
    /// Clamp values from the UI or a hand-edited config to sane ranges.
    pub fn sanitized(mut self) -> Self {
        let clamp = |v: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { 0.0f32.clamp(lo, hi) };
        self.gain_db = clamp(self.gain_db, -30.0, 30.0);
        self.gate_db = clamp(self.gate_db, -90.0, 0.0);
        self.bass_db = clamp(self.bass_db, -15.0, 15.0);
        self.presence_db = clamp(self.presence_db, -15.0, 15.0);
        self.treble_db = clamp(self.treble_db, -15.0, 15.0);
        self.compressor = clamp(self.compressor, 0.0, 1.0);
        self.pitch = clamp(self.pitch, -12.0, 12.0);
        self.voice_mix = clamp(self.voice_mix, 0.0, 1.0);
        self.noise_strength = self.noise_strength.clamp(1, 4);
        self
    }
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
        match toml::from_str::<Self>(&text) {
            Ok(mut cfg) => {
                // The first effects builds keyed microphones by bare node.name.
                cfg.fx = std::mem::take(&mut cfg.fx)
                    .into_iter()
                    .map(|(k, v)| if FxTarget::parse(&k).is_some() { (k, v) } else { (format!("mic:{k}"), v) })
                    .collect();
                Some(cfg)
            }
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
            fx: BTreeMap::new(),
            apps: BTreeMap::new(),
        }
    }

    /// Effects of the strip with this key, if they are switched on.
    pub fn active_fx(&self, key: &str) -> Option<&Effects> {
        self.fx.get(key).filter(|fx| fx.enabled)
    }

    /// Keys of every strip whose effects are switched on.
    pub fn active_fx_keys(&self) -> impl Iterator<Item = &str> {
        self.fx.iter().filter(|(_, fx)| fx.enabled).map(|(k, _)| k.as_str())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kept_apps_survive_a_round_trip() {
        let mut cfg = Config::first_run(None);
        cfg.apps.insert("app:Some Game".into(), KnownApp { label: "Some Game".into(), icon: String::new(), kept: true });
        cfg.routes.insert("app:Some Game".into(), vec!["main".into()]);
        let text = toml::to_string_pretty(&cfg).unwrap();
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), cfg);
    }
}
