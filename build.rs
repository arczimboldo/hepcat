use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("org.hepcat").qml_files([
        "qml/Main.qml",
        "qml/Strip.qml",
        "qml/Section.qml",
    ]))
    .qt_module("Gui")
    .qt_module("Widgets")
    .qt_module("Quick")
    .qt_module("QuickControls2")
    .qt_module("Network")
    .qt_module("Svg")
    .qrc_resources(["icons/hepcat.svg", "icons/hepcat-symbolic.svg"])
    .files(["src/bridge.rs"])
    .cpp_file("cpp/launcher.cpp")
    .build();
}
