//! The QML-facing `Mixer` object. It owns the channel to the PipeWire thread
//! and exposes the latest graph snapshot as a JSON string property.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("hepcat/cpp/launcher.h");
        /// Create the QApplication, tray icon and QML engine and run the event loop.
        #[cxx_name = "runApp"]
        fn run_app(start_hidden: bool) -> i32;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, state_json, cxx_name = "stateJson")]
        #[qproperty(QString, levels_json, cxx_name = "levelsJson")]
        type Mixer = super::MixerRust;

        #[qinvokable]
        #[cxx_name = "setVolume"]
        fn set_volume(self: &Mixer, ids: &QString, volume: f64);

        #[qinvokable]
        #[cxx_name = "setMute"]
        fn set_mute(self: &Mixer, ids: &QString, mute: bool);

        #[qinvokable]
        #[cxx_name = "toggleRoute"]
        fn toggle_route(self: &Mixer, key: &QString, bus: &QString);

        #[qinvokable]
        #[cxx_name = "setBusOutput"]
        fn set_bus_output(self: &Mixer, bus: &QString, output: &QString);

        #[qinvokable]
        #[cxx_name = "setBusMic"]
        fn set_bus_mic(self: &Mixer, bus: &QString, enabled: bool);

        #[qinvokable]
        #[cxx_name = "addBus"]
        fn add_bus(self: &Mixer, label: &QString);

        #[qinvokable]
        #[cxx_name = "removeBus"]
        fn remove_bus(self: &Mixer, name: &QString);

        #[qinvokable]
        #[cxx_name = "setMetersEnabled"]
        fn set_meters_enabled(self: &Mixer, enabled: bool);

        #[qinvokable]
        #[cxx_name = "setDefaultSink"]
        fn set_default_sink(self: &Mixer, name: &QString);
    }

    impl cxx_qt::Threading for Mixer {}
    impl cxx_qt::Initialize for Mixer {}
}

use crate::engine::{self, Command};
use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;

pub use qobject::run_app;

#[derive(Default)]
pub struct MixerRust {
    state_json: QString,
    levels_json: QString,
    sender: Option<engine::Sender>,
}

impl cxx_qt::Initialize for qobject::Mixer {
    fn initialize(mut self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        let sender = engine::spawn(move |event| {
            // Fails only if the Mixer is already gone, in which case nobody is listening.
            let _ = match event {
                engine::Event::State(json) => qt_thread.queue(move |m| m.set_state_json(QString::from(&json))),
                engine::Event::Levels(json) => qt_thread.queue(move |m| m.set_levels_json(QString::from(&json))),
            };
        });
        self.as_mut().rust_mut().get_mut().sender = Some(sender);
    }
}

/// Node ids travel from QML as a comma separated string.
fn parse_ids(ids: &QString) -> Vec<u32> {
    ids.to_string().split(',').filter_map(|s| s.trim().parse().ok()).collect()
}

impl qobject::Mixer {
    fn send(&self, cmd: Command) {
        if let Some(sender) = &self.rust().sender {
            let _ = sender.send(cmd);
        }
    }

    fn set_volume(&self, ids: &QString, volume: f64) {
        self.send(Command::SetVolume { ids: parse_ids(ids), volume });
    }

    fn set_mute(&self, ids: &QString, mute: bool) {
        self.send(Command::SetMute { ids: parse_ids(ids), mute });
    }

    fn toggle_route(&self, key: &QString, bus: &QString) {
        self.send(Command::ToggleRoute { key: key.to_string(), bus: bus.to_string() });
    }

    fn set_bus_output(&self, bus: &QString, output: &QString) {
        let output = output.to_string();
        let output = (!output.is_empty()).then_some(output);
        self.send(Command::SetBusOutput { bus: bus.to_string(), output });
    }

    fn set_bus_mic(&self, bus: &QString, enabled: bool) {
        self.send(Command::SetBusMic { bus: bus.to_string(), enabled });
    }

    fn add_bus(&self, label: &QString) {
        self.send(Command::AddBus { label: label.to_string() });
    }

    fn remove_bus(&self, name: &QString) {
        self.send(Command::RemoveBus { name: name.to_string() });
    }

    fn set_meters_enabled(&self, enabled: bool) {
        self.send(Command::SetMeters(enabled));
    }

    fn set_default_sink(&self, name: &QString) {
        self.send(Command::SetDefaultSink { name: name.to_string() });
    }
}
