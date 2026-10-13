<p align="center"><img src="icons/hepcat.svg" width="128" alt="Hepcat"></p>

# Hepcat

*A cool cat for your audio.* A Voicemeeter-style mixer for PipeWire with a
native KDE Plasma look.

Instead of a node graph, Hepcat shows mixer strips:

- **Microphones** and **Applications** are sources. Each has a fader, mute and
  one button per bus. Click buttons to send the source to one or more buses.
- **Buses** are virtual outputs that Hepcat creates (they appear as normal
  output devices, e.g. "Main (Hepcat)"). Each bus has a fader and a picker
  for the hardware device it plays to. Star a bus to make it the system
  default, so every new app plays into it.
- **Outputs** are your real devices.
- Every strip has a **level meter** next to its fader.
- The 🎤 button on a bus offers it as a **microphone** ("Chat (Hepcat mic)"),
  so Discord, OBS or a browser can record exactly what goes into that bus.

Closing the window keeps Hepcat running in the **system tray**, so rules keep
being applied (for example when headphones reconnect). Use "Start at Login" in
the tray menu to launch it hidden at login; "Quit" stops it. Starting Hepcat
again while it runs just shows the window.

Routing is saved as rules keyed by app name or device name in
`~/.config/hepcat/config.toml`. Rules are reapplied whenever an app
restarts or a device comes back, so nothing has to be rewired by hand.

## How it works

- Rust backend talking to PipeWire directly (`pipewire-rs`), UI in QML with
  Kirigami via [cxx-qt](https://github.com/KDAB/cxx-qt).
- An app's first bus is set through WirePlumber's `target.object` metadata,
  exactly like moving a stream in the Plasma applet. Extra buses are added
  as direct links. Hepcat never deletes links WirePlumber made; it moves
  streams by retargeting them instead.
- Buses are `support.null-audio-sink` nodes created with `object.linger`, so
  they and their links keep working after the window is closed.
- Volume and mute go through `wpctl`, so hardware devices get their real
  route volume and WirePlumber remembers it.
- Meters are small capture streams linked passively (they never wake a device
  or a microphone) with their own media class, so they don't show up as
  recording apps in the Plasma volume applet. They pause while the window is
  hidden.
- Bus microphones are `Audio/Source/Virtual` null nodes fed from the bus
  monitor.

## Build and run

Needs Rust, Qt 6 (base, declarative), Kirigami, qqc2-desktop-style,
PipeWire and WirePlumber. On Arch:
`pacman -S rust qt6-declarative kirigami qqc2-desktop-style pipewire wireplumber`.

```sh
cargo run --release
```

Install for your user:

```sh
cargo install --path .
cp org.hepcat.Hepcat.desktop ~/.local/share/applications/
install -Dm644 icons/hepcat.svg ~/.local/share/icons/hicolor/scalable/apps/org.hepcat.Hepcat.svg
```

## Development

```sh
cargo test                       # routing rules (src/engine/graph.rs)
HEPCAT_DEBUG=1 cargo run        # print every state snapshot
echo "toggle app:Firefox main" | cargo run -- --headless   # no window
HEPCAT_NO_METERS=1 cargo run    # disable level meters
QT_QPA_PLATFORM=offscreen HEPCAT_SCREENSHOT=shot.png cargo run   # render UI to a file
```

`--headless` reads one command per line: `toggle <key> <bus>`,
`output <bus> [device]`, `volume <ids> <0..1.5>`, `mute <ids> 0|1`, `mic <bus> 0|1`,
`add-bus <label>`, `remove-bus <name>`, `default <node.name>`.

Coming from the Mixwire builds: the config moves to `~/.config/hepcat`
automatically, and the old "(Mixwire)" buses are replaced by "(Hepcat)" ones
(your default output carries over) once no old Mixwire process is running.

To remove leftover buses after uninstalling: `pw-cli destroy <id>` for each
"(Hepcat)" sink listed by `wpctl status`, or restart PipeWire.

## Roadmap

- Rename buses, reorder strips
- Show saved routes for apps that are not running
- Per-strip effects (noise suppression, EQ) via filter-chain
- Optional filtered graph view for advanced patching
