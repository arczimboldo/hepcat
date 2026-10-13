//! Volume and mute changes are applied through `wpctl`, which goes through
//! WirePlumber's mixer API. That way hardware devices get their volume set on
//! the active route (like the Plasma applet does) and WirePlumber remembers it.
//!
//! Slider drags produce a flood of updates, so a worker thread collapses them
//! to the latest value per node before running anything.

use std::collections::BTreeMap;
use std::process::Command;
use std::sync::mpsc;

#[derive(Debug, Clone, Copy)]
pub enum Change {
    Volume(f64),
    Mute(bool),
}

pub struct VolumeWorker {
    tx: mpsc::Sender<(u32, Change)>,
}

impl VolumeWorker {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<(u32, Change)>();
        std::thread::Builder::new()
            .name("hepcat-volume".into())
            .spawn(move || {
                while let Ok(first) = rx.recv() {
                    let mut volumes = BTreeMap::new();
                    let mut mutes = BTreeMap::new();
                    for (id, change) in std::iter::once(first).chain(rx.try_iter()) {
                        match change {
                            Change::Volume(v) => {
                                volumes.insert(id, v);
                            }
                            Change::Mute(m) => {
                                mutes.insert(id, m);
                            }
                        }
                    }
                    for (id, v) in volumes {
                        wpctl(&["set-volume", &id.to_string(), &format!("{:.4}", v.clamp(0.0, 1.5))]);
                    }
                    for (id, m) in mutes {
                        wpctl(&["set-mute", &id.to_string(), if m { "1" } else { "0" }]);
                    }
                }
            })
            .expect("failed to spawn volume thread");
        Self { tx }
    }

    pub fn send(&self, id: u32, change: Change) {
        let _ = self.tx.send((id, change));
    }
}

fn wpctl(args: &[&str]) {
    match Command::new("wpctl").args(args).status() {
        Ok(s) if s.success() => {}
        Ok(s) => eprintln!("hepcat: wpctl {} exited with {s}", args.join(" ")),
        Err(e) => eprintln!("hepcat: failed to run wpctl: {e}"),
    }
}
