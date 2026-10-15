<p align="center"><img src="icons/hepcat.svg" width="128" alt="Hepcat"></p>

# Hepcat

*A cool cat for your audio.* A Voicemeeter-style mixer for PipeWire with a
native KDE Plasma look.

## Install

Hepcat needs a desktop with PipeWire, which is the default on most current
distributions (Fedora, Ubuntu 22.10+, Debian 12+, Arch, openSUSE, Mint 22+).

**Any distribution (Flatpak).** Download `hepcat.flatpak` from the
[latest release](https://github.com/arczimboldo/hepcat/releases/latest) and
open it, or install it from a terminal:

```sh
flatpak install --user hepcat.flatpak
```

If Flatpak is new to you, set it up for your distribution first:
<https://flathub.org/setup>.

**Arch Linux and derivatives** (CachyOS, EndeavourOS, Manjaro):

```sh
git clone https://github.com/arczimboldo/hepcat
cd hepcat/packaging/arch/hepcat-git
makepkg -si
```

**Other distributions, from source:** see [Build and run](#build-and-run).

## What it does

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
- The ⚙ button on a microphone, app or bus opens its **effects**:
  - On a **microphone**, switching effects on adds a processed copy of the
    mic, "Headset (Hepcat FX)". Buses the mic is sent to get the processed
    sound, and it becomes the default microphone if the raw one was.
  - An **app** plays into "Firefox (Hepcat FX)" instead, and the processed
    sound goes to its buses, or to the default output if it has none. Use it
    to clean up a noisy voice in a call, even out a quiet video, or EQ music.
  - A **bus** is processed before it reaches its output and its microphone.

  The effects are:
  - **Noise suppression.** *Standard* removes steady noise (fans, hum, hiss)
    and lets every other sound through, claps and music included. *Voice
    isolation* uses RNNoise, a neural network, and keeps only speech, much
    like Discord's Krisp.
  - **Echo cancellation** (microphones only). Removes what your speakers play
    from the mic.
  - **Noise gate.**
  - **Gain**, automatic gain control and a compressor.
  - **Tone:** bass, presence and treble.
  - **Voice changer:** pitch shift plus robot, alien, radio, telephone,
    megaphone, underwater, echo and hall effects, with a wet/dry mix.
- The ☆ on a microphone makes it the default input.

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
- Volume and mute are set the way WirePlumber's own mixer does it: on the
  active route for hardware devices (so the real mixer control moves and
  WirePlumber remembers it per port), on the node for everything else.
- Hepcat identifies itself as an audio manager (`media.category = Manager`,
  like pavucontrol), which inside a Flatpak lets it move streams and change
  the default devices.
- Meters are small capture streams linked passively (they never wake a device
  or a microphone) with their own media class, so they don't show up as
  recording apps in the Plasma volume applet. They pause while the window is
  hidden.
- Bus microphones are `Audio/Source/Virtual` null nodes fed from the bus
  monitor.
- Effects run inside Hepcat, so they need it running (the tray is enough).
  Without Hepcat, an app with effects falls back to the default output and a
  bus with effects is silent until Hepcat starts again. A capture stream reads
  the mic (or the app's virtual output, or the bus monitor), and the chain
  processes 10 ms frames, mono for mics and stereo for apps and buses. The
  streams run on PipeWire's realtime thread and share one driver.
  WebRTC's audio processing (the same library PipeWire's echo-cancel module
  uses) handles echo, standard noise suppression and AGC. `nnnoiseless`
  handles voice isolation, and the rest is plain Rust DSP. A playback stream
  feeds the result into a null `Audio/Source/Virtual` node. For echo
  cancellation, a passive capture of the default output's monitor is the
  reference; when the default output is a bus, Hepcat uses that bus's device,
  so every bus that plays there is cancelled.

## Build and run

Needs Rust 1.85+, clang, Qt 6 (base, declarative, svg), Kirigami and
qqc2-desktop-style for Qt 6 (KDE Frameworks 6), PipeWire, WirePlumber and
webrtc-audio-processing 1.x or 2.x (a PipeWire dependency). That rules out
Ubuntu 24.04 and Debian 12, which lack KDE Frameworks 6; use the Flatpak there.

```sh
# Arch
sudo pacman -S --needed rust clang qt6-declarative qt6-svg kirigami qqc2-desktop-style \
  pipewire wireplumber webrtc-audio-processing-1
# Fedora
sudo dnf install cargo clang gcc-c++ qt6-qtbase-devel qt6-qtdeclarative-devel qt6-qtsvg-devel \
  kf6-kirigami-devel kf6-qqc2-desktop-style pipewire-devel webrtc-audio-processing-devel
# Debian 13, Ubuntu 25.04+
sudo apt install cargo clang g++ pkg-config qt6-base-dev qt6-declarative-dev qt6-svg-dev \
  qml6-module-org-kde-kirigami qml6-module-org-kde-qqc2desktopstyle \
  libpipewire-0.3-dev libwebrtc-audio-processing-dev
```

Only the Arch list is tested; package names elsewhere may differ slightly.

```sh
cargo run --release
```

Install for your user:

```sh
cargo install --path .
cp io.github.arczimboldo.Hepcat.desktop ~/.local/share/applications/
install -Dm644 icons/hepcat.svg ~/.local/share/icons/hicolor/scalable/apps/io.github.arczimboldo.Hepcat.svg
```

## Development

```sh
cargo test                       # routing rules and the effects DSP
HEPCAT_DEBUG=1 cargo run        # print every state snapshot
echo "toggle app:Firefox main" | cargo run -- --headless   # no window
HEPCAT_NO_METERS=1 cargo run    # disable level meters
QT_QPA_PLATFORM=offscreen HEPCAT_SCREENSHOT=shot.png cargo run   # render UI to a file
```

`--headless` reads one command per line: `toggle <key> <bus>`,
`output <bus> [device]`, `volume <ids> <0..1.5>`, `mute <ids> 0|1`, `mic <bus> 0|1`,
`add-bus <label>`, `remove-bus <name>`, `default <node.name>`,
`default-source <node.name>`, `fx <key> <json>`, where the key is
`mic:<node.name>`, `app:<name>` or `bus:<name>` (for example
`fx mic:alsa_input.usb-mic {"enabled":true,"noise":"voice","pitch":-4}`).

Coming from the Mixwire builds: the config moves to `~/.config/hepcat`
automatically, and the old "(Mixwire)" buses are replaced by "(Hepcat)" ones
(your default output carries over) once no old Mixwire process is running.

To remove leftover buses after uninstalling: `pw-cli destroy <id>` for each
"(Hepcat)" sink listed by `wpctl status`, or restart PipeWire
(`systemctl --user restart pipewire wireplumber`).

Packaging (Arch, Flatpak, releases): see [packaging/README.md](packaging/README.md).

## License

MIT, see [LICENSE](LICENSE).

## Roadmap

- Rename buses, reorder strips
- Show saved routes for apps that are not running
- Effects on hardware outputs
- Optional filtered graph view for advanced patching
