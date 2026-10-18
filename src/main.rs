mod bridge;
mod config;
mod engine;

fn main() {
    if std::env::args().any(|a| a == "--headless") {
        headless();
        return;
    }
    cxx_qt::init_qml_module!("org.hepcat");
    let minimized = std::env::args().any(|a| a == "--minimized");
    std::process::exit(bridge::run_app(minimized));
}

/// Run the engine without a window, reading UI commands from stdin, one per
/// line, and printing every state snapshot. Handy for scripting and testing:
///
/// ```text
/// toggle app:Firefox main
/// output main alsa_output.usb-headset
/// volume 42,43 0.5
/// mic chat 1
/// fx mic:alsa_input.usb-mic {"enabled":true,"noise":"voice","pitch":-4}
/// fx app:Firefox {"enabled":true,"auto_gain":true}
/// ```
fn headless() {
    use engine::Command;
    use std::io::BufRead;

    let sender = engine::spawn(|event| {
        if let engine::Event::State(json) = event {
            println!("{json}");
        }
    });
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        let words: Vec<&str> = line.split_whitespace().collect();
        let ids = |s: &str| s.split(',').filter_map(|i| i.parse().ok()).collect();
        let cmd = match words.as_slice() {
            ["toggle", key, bus] => Command::ToggleRoute { key: key.to_string(), bus: bus.to_string() },
            ["output", bus] => Command::SetBusOutput { bus: bus.to_string(), output: None },
            ["output", bus, out] => Command::SetBusOutput { bus: bus.to_string(), output: Some(out.to_string()) },
            ["volume", i, v] => match v.parse() {
                Ok(volume) => Command::SetVolume { ids: ids(i), volume },
                Err(_) => continue,
            },
            ["mute", i, m] => Command::SetMute { ids: ids(i), mute: *m == "1" || *m == "true" },
            ["mic", bus, m] => Command::SetBusMic { bus: bus.to_string(), enabled: *m == "1" || *m == "true" },
            ["add-bus", label @ ..] => Command::AddBus { label: label.join(" ") },
            ["remove-bus", name] => Command::RemoveBus { name: name.to_string() },
            ["rename-bus", name, label @ ..] => Command::RenameBus { name: name.to_string(), label: label.join(" ") },
            ["add-app", name @ ..] => Command::AddApp { name: name.join(" ") },
            ["forget-app", key] => Command::ForgetApp { key: key.to_string() },
            ["default", name] => Command::SetDefaultSink { name: name.to_string() },
            ["default-source", name] => Command::SetDefaultSource { name: name.to_string() },
            ["fx", key, json @ ..] => match serde_json::from_str(&json.join(" ")) {
                Ok(fx) => Command::SetFx { key: key.to_string(), fx },
                Err(e) => {
                    eprintln!("hepcat: bad effects JSON: {e}");
                    continue;
                }
            },
            [] => continue,
            _ => {
                eprintln!("hepcat: unknown command: {line}");
                continue;
            }
        };
        let _ = sender.send(cmd);
    }
}
