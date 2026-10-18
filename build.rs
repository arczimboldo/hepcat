use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    let kglobalaccel = kglobalaccel_include();
    let builder = CxxQtBuilder::new_qml_module(QmlModule::new("org.hepcat").qml_files([
        "qml/Main.qml",
        "qml/Strip.qml",
        "qml/Section.qml",
        "qml/Fader.qml",
        "qml/EffectsDialog.qml",
        "qml/AddTile.qml",
        "qml/AddAppDialog.qml",
        "qml/Shortcuts.qml",
        "qml/ShortcutsDialog.qml",
    ]));
    // SAFETY: only adds an include path, which cannot clash with what
    // cxx-qt-build sets up itself.
    let builder = unsafe {
        builder.cc_builder(move |cc| {
            cc.include(&kglobalaccel);
        })
    };
    builder
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
        .cpp_file("cpp/shortcuts.cpp")
        .build();
    println!("cargo:rustc-link-lib=KF6GlobalAccel");

    build_apm();
}

/// KF6's GlobalAccel ships CMake files but no pkg-config file, so look for
/// its headers where distributions (and the KDE Flatpak SDK) put them.
fn kglobalaccel_include() -> std::path::PathBuf {
    println!("cargo:rerun-if-env-changed=KF6_INCLUDE_DIR");
    let roots = std::env::var("KF6_INCLUDE_DIR").into_iter().chain(["/usr/include/KF6".into(), "/usr/local/include/KF6".into()]);
    roots
        .map(|root| std::path::Path::new(&root).join("KGlobalAccel"))
        .find(|dir| dir.join("kglobalaccel.h").exists())
        .expect("KGlobalAccel headers not found; install KF6 GlobalAccel (kglobalaccel) or set KF6_INCLUDE_DIR")
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
