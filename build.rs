use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("org.hepcat").qml_files([
        "qml/Main.qml",
        "qml/Strip.qml",
        "qml/Section.qml",
        "qml/Fader.qml",
        "qml/EffectsDialog.qml",
    ]))
    .qt_module("Gui")
    .qt_module("Widgets")
    .qt_module("Quick")
    .qt_module("QuickControls2")
    .qt_module("Network")
    .qt_module("DBus")
    .qt_module("Svg")
    .qrc_resources(["icons/hepcat.svg", "icons/hepcat-symbolic.svg"])
    .files(["src/bridge.rs"])
    .cpp_file("cpp/launcher.cpp")
    .build();

    build_apm();
}

/// The WebRTC audio processing shim (echo cancellation, noise suppression,
/// automatic gain), against whichever webrtc-audio-processing is installed:
/// 1.x (Arch, Fedora 41) or 2.x (newer distros, the Flatpak runtimes). Our
/// static library has to come before the system one on the link line, so it
/// is compiled before pkg-config prints its link flags.
fn build_apm() {
    let found = ["webrtc-audio-processing-1", "webrtc-audio-processing-2"]
        .into_iter()
        .find_map(|name| Some((name, pkg_config::Config::new().cargo_metadata(false).probe(name).ok()?)));
    let Some((name, lib)) = found else {
        panic!("webrtc-audio-processing (1.x or 2.x) not found; it ships with PipeWire, install its development package");
    };
    let mut cc = cc::Build::new();
    cc.cpp(true).std("c++17").file("cpp/apm.cpp").warnings(false);
    for path in &lib.include_paths {
        cc.include(path);
    }
    for (define, value) in &lib.defines {
        cc.define(define, value.as_deref());
    }
    cc.compile("hepcat_apm");
    pkg_config::Config::new().probe(name).expect("pkg-config failed the second time");
    println!("cargo:rerun-if-changed=cpp/apm.cpp");
}
