// Effects for one microphone, app or bus: cleanup, gain, tone and voice changer.

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Kirigami.Dialog {
    id: dialog

    required property var mixer
    // `mic:…`, `app:…` or `bus:…`
    property string key
    // "mic", "app" or "bus"
    property string kind
    property string label
    // Local copy of the strip's effects; every change is sent right away.
    property var settings: ({})
    readonly property bool isMic: kind === "mic"

    function openFor(key, kind, label, fxJson) {
        dialog.key = key
        dialog.kind = kind
        dialog.label = label
        settings = JSON.parse(fxJson || "{}")
        open()
    }

    function set(name, value) {
        const s = Object.assign({}, settings)
        s[name] = value
        settings = s
        mixer.setFx(dialog.key, JSON.stringify(s))
    }

    function resetEffects() {
        settings = { enabled: settings.enabled, gain_db: 0, auto_gain: false, noise: "off", noise_strength: 3, echo: false,
                     gate: false, gate_db: -50, bass_db: 0, presence_db: 0, treble_db: 0, compressor: 0,
                     pitch: 0, voice: "none", voice_mix: 1 }
        mixer.setFx(dialog.key, JSON.stringify(settings))
    }

    readonly property bool on: settings.enabled === true

    title: "Effects – " + label
    preferredWidth: Kirigami.Units.gridUnit * 30
    padding: Kirigami.Units.largeSpacing
    standardButtons: Kirigami.Dialog.Close
    customFooterActions: [
        Kirigami.Action {
            text: "Reset"
            icon.name: "edit-reset"
            enabled: dialog.on
            onTriggered: dialog.resetEffects()
        }
    ]

    // A slider with its value printed next to it.
    component ValueSlider: RowLayout {
        id: vs
        property string key
        property real from
        property real to
        property real stepSize: 1
        property real fallback: 0
        property var format: v => Math.round(v)
        Layout.fillWidth: true
        enabled: dialog.on

        QQC2.Slider {
            id: s
            from: vs.from
            to: vs.to
            stepSize: vs.stepSize
            Layout.fillWidth: true
            Layout.minimumWidth: Kirigami.Units.gridUnit * 10
            onMoved: dialog.set(vs.key, value)
            Binding on value {
                value: dialog.settings[vs.key] ?? vs.fallback
                when: !s.pressed
                restoreMode: Binding.RestoreNone
            }
        }
        QQC2.Label {
            text: vs.format(s.value)
            font.features: { "tnum": 1 }
            horizontalAlignment: Text.AlignRight
            Layout.preferredWidth: Kirigami.Units.gridUnit * 3.5
        }
    }

    readonly property var db: v => (v > 0 ? "+" : "") + v.toFixed(0) + " dB"

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        QQC2.Switch {
            text: ({ mic: "Process this microphone", app: "Process this app", bus: "Process this bus" })[dialog.kind] ?? "Process"
            checked: dialog.on
            onToggled: dialog.set("enabled", checked)
        }
        QQC2.Label {
            text: ({
                mic: "Adds a microphone called “" + dialog.label + " (Hepcat FX)” for Discord, OBS and other apps. "
                    + "Buses this mic is sent to get the processed sound.",
                app: "The app plays into “" + dialog.label + " (Hepcat FX)”, and the processed sound goes to its buses, "
                    + "or to the default output when it has none.",
                bus: "Everything in this bus is processed before it reaches its output and its microphone.",
            })[dialog.kind] + " Effects run while Hepcat is running, also from the tray."
            wrapMode: Text.WordWrap
            opacity: 0.7
            font: Kirigami.Theme.smallFont
            Layout.fillWidth: true
            // Keep the long text from stretching the dialog.
            Layout.preferredWidth: Kirigami.Units.gridUnit * 26
        }

        Kirigami.FormLayout {
            Layout.fillWidth: true

            Kirigami.Separator {
                Kirigami.FormData.isSection: true
                Kirigami.FormData.label: "Cleanup"
            }

            QQC2.ComboBox {
                id: noiseBox
                Kirigami.FormData.label: "Noise suppression:"
                enabled: dialog.on
                textRole: "label"
                valueRole: "value"
                model: [
                    { value: "off", label: "Off" },
                    { value: "standard", label: "Standard" },
                    { value: "voice", label: "Voice isolation" },
                ]
                currentIndex: Math.max(0, model.findIndex(m => m.value === (dialog.settings.noise || "off")))
                onActivated: dialog.set("noise", currentValue)
            }
            QQC2.Label {
                text: ({
                    off: "No noise removal.",
                    standard: "Removes steady background noise like fans, hum and hiss. Every other sound gets through: claps, instruments, music.",
                    voice: "AI noise removal: keeps only speech and removes everything else, including keyboard clicks, dogs and claps.",
                })[dialog.settings.noise || "off"]
                wrapMode: Text.WordWrap
                opacity: 0.7
                font: Kirigami.Theme.smallFont
                Layout.maximumWidth: Kirigami.Units.gridUnit * 16
            }
            ValueSlider {
                id: strengthSlider
                Kirigami.FormData.label: "Strength:"
                visible: (dialog.settings.noise || "off") !== "off"
                enabled: dialog.on
                key: "noise_strength"
                from: 1
                to: 4
                fallback: 3
                format: v => ["Light", "Medium", "Strong", "Maximum"][Math.round(v) - 1]
                QQC2.ToolTip.text: dialog.settings.noise === "voice"
                    ? "Light and Medium leave some background in, so speech sounds more natural. "
                      + "Strong removes all the noise it can. Maximum also mutes everything while you aren't talking."
                    : "How far steady noise is pushed down: from about 6 dB (Light) to about 21 dB (Maximum). "
                      + "Stronger settings can make speech sound thinner."
                QQC2.ToolTip.visible: strengthHover.hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                HoverHandler { id: strengthHover }
            }

            QQC2.CheckBox {
                Kirigami.FormData.label: "Echo:"
                text: "Remove sound from my speakers"
                enabled: dialog.on
                checked: dialog.settings.echo === true
                onToggled: dialog.set("echo", checked)
                QQC2.ToolTip.text: "Echo cancellation: whatever plays on the default output is removed from "
                    + (dialog.isMic ? "the microphone" : "this " + dialog.kind + "'s sound, for when it carries a microphone (a phone mirrored with scrcpy, for example)")
                    + ", so people don't hear themselves. Its own sound is never removed, even when you listen to it."
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            }

            QQC2.CheckBox {
                id: gateBox
                Kirigami.FormData.label: "Noise gate:"
                text: "Mute when quieter than"
                enabled: dialog.on
                checked: dialog.settings.gate === true
                onToggled: dialog.set("gate", checked)
            }
            ValueSlider {
                key: "gate_db"
                from: -90
                to: 0
                fallback: -50
                format: v => v.toFixed(0) + " dB"
                enabled: dialog.on && gateBox.checked
            }

            Kirigami.Separator {
                Kirigami.FormData.isSection: true
                Kirigami.FormData.label: "Level"
            }

            ValueSlider {
                Kirigami.FormData.label: "Gain:"
                key: "gain_db"
                from: -30
                to: 30
                format: dialog.db
            }
            QQC2.CheckBox {
                text: "Automatic gain control"
                enabled: dialog.on
                checked: dialog.settings.auto_gain === true
                onToggled: dialog.set("auto_gain", checked)
                QQC2.ToolTip.text: "Turns quiet speech up and loud speech down, so you are always heard at the same volume."
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
            ValueSlider {
                Kirigami.FormData.label: "Compressor:"
                key: "compressor"
                from: 0
                to: 1
                stepSize: 0.05
                format: v => v === 0 ? "Off" : Math.round(v * 100) + "%"
            }

            Kirigami.Separator {
                Kirigami.FormData.isSection: true
                Kirigami.FormData.label: "Tone"
            }

            ValueSlider {
                Kirigami.FormData.label: "Bass:"
                key: "bass_db"
                from: -15
                to: 15
                format: dialog.db
            }
            ValueSlider {
                Kirigami.FormData.label: "Presence:"
                key: "presence_db"
                from: -15
                to: 15
                format: dialog.db
            }
            ValueSlider {
                Kirigami.FormData.label: "Treble:"
                key: "treble_db"
                from: -15
                to: 15
                format: dialog.db
            }

            Kirigami.Separator {
                Kirigami.FormData.isSection: true
                Kirigami.FormData.label: "Voice changer"
            }

            ValueSlider {
                Kirigami.FormData.label: "Pitch:"
                key: "pitch"
                from: -12
                to: 12
                stepSize: 0.5
                format: v => v === 0 ? "Off" : (v > 0 ? "+" : "") + v + " st"
            }
            RowLayout {
                enabled: dialog.on
                Repeater {
                    model: [{ label: "Deep", pitch: -5 }, { label: "Normal", pitch: 0 }, { label: "Chipmunk", pitch: 7 }]
                    delegate: QQC2.Button {
                        required property var modelData
                        text: modelData.label
                        flat: true
                        onClicked: dialog.set("pitch", modelData.pitch)
                    }
                }
            }
            QQC2.ComboBox {
                Kirigami.FormData.label: "Effect:"
                enabled: dialog.on
                textRole: "label"
                valueRole: "value"
                model: [
                    { value: "none", label: "None" },
                    { value: "robot", label: "Robot" },
                    { value: "alien", label: "Alien" },
                    { value: "radio", label: "Radio" },
                    { value: "telephone", label: "Telephone" },
                    { value: "megaphone", label: "Megaphone" },
                    { value: "underwater", label: "Underwater" },
                    { value: "echo", label: "Echo" },
                    { value: "hall", label: "Hall" },
                ]
                currentIndex: Math.max(0, model.findIndex(m => m.value === (dialog.settings.voice || "none")))
                onActivated: dialog.set("voice", currentValue)
            }
            ValueSlider {
                Kirigami.FormData.label: "Effect mix:"
                key: "voice_mix"
                from: 0
                to: 1
                stepSize: 0.05
                fallback: 1
                format: v => Math.round(v * 100) + "%"
                enabled: dialog.on && (dialog.settings.voice || "none") !== "none"
            }
        }
    }
}
