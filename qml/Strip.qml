// One mixer channel: an app, microphone, bus or hardware output.

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

QQC2.Control {
    id: strip

    // "app", "mic", "bus" or "output"
    required property string kind
    required property var mixer
    property string key
    property string label
    property string detail
    property string icon
    property string ids
    property real volume
    // Peak level 0..1 (-60..0 dB)
    property real level
    property bool mute
    // "|main|chat|" style list of buses this source currently feeds
    property string buses
    property string direct
    property bool isDefault
    // Bus-only
    property string busName
    property string nodeName
    property string output
    property string outputLabel
    property bool online: true
    property bool micEnabled
    // Mic-only: node.name of the mic and of its processed copy
    property string micName
    property string fxDevice
    // Effects (mics, apps and buses): settings as JSON, and whether they run
    property string fx
    property bool fxActive
    readonly property var fxSettings: fx ? JSON.parse(fx) : ({})
    readonly property bool fxEnabled: fxSettings.enabled === true

    // Human-readable list of the effects that actually change the sound.
    function fxSummary() {
        const f = fxSettings
        const db = v => (v > 0 ? "+" : "") + Math.round(v) + " dB"
        const parts = []
        if (f.noise === "standard") parts.push("Noise suppression")
        if (f.noise === "voice") parts.push("Voice isolation")
        if (f.echo) parts.push("Echo cancellation")
        if (f.gate) parts.push("Noise gate (" + Math.round(f.gate_db) + " dB)")
        if (f.gain_db) parts.push("Gain " + db(f.gain_db))
        if (f.auto_gain) parts.push("Automatic gain")
        if (f.compressor > 0) parts.push("Compressor " + Math.round(f.compressor * 100) + "%")
        const eq = [["Bass", f.bass_db], ["Presence", f.presence_db], ["Treble", f.treble_db]]
            .filter(e => e[1]).map(e => e[0] + " " + db(e[1]))
        if (eq.length) parts.push(eq.join(", "))
        if (f.pitch) parts.push("Pitch " + (f.pitch > 0 ? "+" : "") + f.pitch + " st")
        if (f.voice && f.voice !== "none")
            parts.push(f.voice.charAt(0).toUpperCase() + f.voice.slice(1) + " effect"
                       + (f.voice_mix < 1 ? " (" + Math.round(f.voice_mix * 100) + "%)" : ""))
        return parts
    }
    // Sources need the bus list to draw their routing buttons, buses need the outputs.
    property var busList
    property var outputChoices: []

    signal removeRequested()
    signal effectsRequested()

    readonly property bool isSource: kind === "app" || kind === "mic"

    HoverHandler { id: stripHover }

    // Wide enough for the button row (buses have four buttons).
    implicitWidth: Math.max(Kirigami.Units.gridUnit * 7, buttonRow.implicitWidth + leftPadding + rightPadding)
    padding: Kirigami.Units.smallSpacing * 2

    Kirigami.Theme.colorSet: Kirigami.Theme.View
    Kirigami.Theme.inherit: false

    background: Rectangle {
        radius: Kirigami.Units.cornerRadius
        color: Kirigami.Theme.backgroundColor
        border.width: 1
        border.color: strip.isDefault
            ? Kirigami.Theme.highlightColor
            : Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.15)
    }

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            Kirigami.Icon {
                source: strip.icon || ({ app: "applications-multimedia", mic: "audio-input-microphone",
                                         bus: "audio-volume-high", output: "audio-card" })[strip.kind]
                fallback: "audio-x-generic"
                Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                Layout.preferredHeight: Kirigami.Units.iconSizes.smallMedium
            }
            QQC2.Label {
                text: strip.label
                font.weight: Font.DemiBold
                elide: Text.ElideRight
                Layout.fillWidth: true
                QQC2.ToolTip.text: strip.label
                QQC2.ToolTip.visible: nameHover.hovered && truncated
                HoverHandler { id: nameHover }
            }
            // Shown while effects are on; hover lists them, click edits them.
            QQC2.AbstractButton {
                id: fxBadge
                visible: strip.fxEnabled
                hoverEnabled: true
                text: "FX"
                padding: 0
                leftPadding: Kirigami.Units.smallSpacing
                rightPadding: Kirigami.Units.smallSpacing
                // Faded while the effects are still starting.
                opacity: strip.fxActive ? 1 : 0.5
                onClicked: strip.effectsRequested()
                contentItem: QQC2.Label {
                    text: fxBadge.text
                    font.pointSize: Kirigami.Theme.smallFont.pointSize
                    font.weight: Font.Bold
                    color: Kirigami.Theme.highlightedTextColor
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
                background: Rectangle {
                    radius: height / 2
                    color: fxBadge.hovered ? Qt.lighter(Kirigami.Theme.highlightColor, 1.15) : Kirigami.Theme.highlightColor
                }
                QQC2.ToolTip.text: {
                    const parts = strip.fxSummary()
                    const head = strip.fxActive ? "Effects on" : "Effects starting…"
                    return parts.length ? head + ":\n• " + parts.join("\n• ") : head + " (no changes yet; click to set them up)"
                }
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
            QQC2.ToolButton {
                visible: strip.kind === "bus"
                opacity: stripHover.hovered || hovered ? 1 : 0
                Behavior on opacity { NumberAnimation { duration: Kirigami.Units.shortDuration } }
                icon.name: "edit-delete"
                display: QQC2.AbstractButton.IconOnly
                text: "Remove bus"
                QQC2.ToolTip.text: text
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                onClicked: strip.removeRequested()
            }
        }

        // Always laid out (even when empty) so faders line up across strips.
        QQC2.Label {
            text: strip.kind === "bus" && !strip.online ? "Starting…"
                : strip.detail !== strip.label ? strip.detail : ""
            elide: Text.ElideRight
            opacity: 0.7
            font: Kirigami.Theme.smallFont
            Layout.fillWidth: true
        }

        RowLayout {
            Layout.fillHeight: true
            Layout.alignment: Qt.AlignHCenter
            Layout.minimumHeight: Kirigami.Units.gridUnit * 6
            spacing: Kirigami.Units.smallSpacing

            QQC2.Slider {
                id: slider
                orientation: Qt.Vertical
                from: 0
                to: 1
                stepSize: 0.01
                Layout.fillHeight: true
                opacity: strip.mute ? 0.5 : 1
                enabled: strip.online
                onMoved: strip.mixer.setVolume(strip.ids, value)

                // Ignore echoes of our own changes while the handle is held.
                Binding on value {
                    value: strip.volume
                    when: !slider.pressed
                    restoreMode: Binding.RestoreNone
                }
            }

            // Peak meter on a -60..0 dB scale: green, yellow from -12 dB, red from -3 dB.
            Rectangle {
                id: meter
                Layout.fillHeight: true
                Layout.topMargin: slider.topPadding
                Layout.bottomMargin: slider.bottomPadding
                implicitWidth: Math.round(Kirigami.Units.smallSpacing * 1.5)
                radius: width / 2
                color: Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.1)
                clip: true

                Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: parent.height * strip.level
                    radius: parent.radius
                    opacity: strip.mute ? 0.4 : 1
                    color: strip.level > 0.95 ? Kirigami.Theme.negativeTextColor
                         : strip.level > 0.8 ? Kirigami.Theme.neutralTextColor
                         : Kirigami.Theme.positiveTextColor
                }
            }
        }

        QQC2.Label {
            text: Math.round(slider.value * 100) + "%"
            font.features: { "tnum": 1 }
            Layout.alignment: Qt.AlignHCenter
        }

        RowLayout {
            id: buttonRow
            Layout.alignment: Qt.AlignHCenter
            spacing: 0
            enabled: strip.online

            QQC2.ToolButton {
                checkable: true
                checked: strip.mute
                icon.name: strip.kind === "mic"
                    ? (strip.mute ? "microphone-sensitivity-muted" : "audio-input-microphone")
                    : (strip.mute ? "audio-volume-muted" : "audio-volume-high")
                text: strip.mute ? "Unmute" : "Mute"
                display: QQC2.AbstractButton.IconOnly
                QQC2.ToolTip.text: text
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                onToggled: strip.mixer.setMute(strip.ids, checked)
            }
            QQC2.ToolButton {
                visible: strip.kind !== "app"
                icon.name: strip.isDefault ? "starred-symbolic" : "non-starred-symbolic"
                text: strip.kind === "mic"
                    ? (strip.isDefault ? "Default microphone" : "Make default microphone")
                    : (strip.isDefault ? "Default output" : "Make default output")
                display: QQC2.AbstractButton.IconOnly
                QQC2.ToolTip.text: strip.kind === "mic"
                    ? (strip.isDefault ? "Apps record from here unless told otherwise"
                                       : "Make this the default microphone" + (strip.fxActive ? ", with its effects" : ""))
                    : (strip.isDefault ? "New apps play here"
                                       : "Make this the default output, so new apps play here")
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                onClicked: {
                    if (strip.kind === "mic")
                        strip.mixer.setDefaultSource(strip.fxActive && strip.fxDevice ? strip.fxDevice : strip.micName)
                    else
                        strip.mixer.setDefaultSink(strip.kind === "bus" ? strip.nodeName : strip.key)
                }
            }
            QQC2.ToolButton {
                visible: strip.kind !== "output"
                checked: strip.fxEnabled
                icon.name: "configure"
                text: "Effects"
                display: QQC2.AbstractButton.IconOnly
                QQC2.ToolTip.text: (strip.fxEnabled ? "Effects are on" : "Effects") + ": "
                    + (strip.kind === "mic" ? "noise suppression, echo cancellation, gain and voice changer"
                                            : "noise suppression, gain, EQ and voice changer")
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                onClicked: strip.effectsRequested()
            }
            QQC2.ToolButton {
                visible: strip.kind === "bus"
                checkable: true
                checked: strip.micEnabled
                icon.name: "audio-input-microphone"
                text: strip.micEnabled ? "Stop offering as microphone" : "Use as microphone"
                display: QQC2.AbstractButton.IconOnly
                QQC2.ToolTip.text: strip.micEnabled
                    ? "Apps can record this bus as “" + strip.label + " (Hepcat mic)”"
                    : "Let apps like Discord or OBS record this bus as a microphone"
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                onToggled: strip.mixer.setBusMic(strip.busName, checked)
            }
        }

        // Sources: which buses this source feeds.
        Flow {
            visible: strip.isSource
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            Repeater {
                model: strip.isSource ? strip.busList : null
                delegate: QQC2.Button {
                    id: routeButton
                    required property string name
                    required property string label
                    readonly property bool active: strip.buses.indexOf("|" + name + "|") >= 0
                    text: label
                    checkable: true
                    checked: active
                    highlighted: active
                    icon.name: active ? "checkmark" : ""
                    width: (parent.width - Kirigami.Units.smallSpacing) / 2
                    QQC2.ToolTip.text: (checked ? "Stop sending to " : "Send to ") + label
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    onClicked: {
                        strip.mixer.toggleRoute(strip.key, name)
                        // Show the real state once the graph confirms it.
                        checked = Qt.binding(() => routeButton.active)
                    }
                }
            }
        }

        QQC2.Label {
            visible: strip.kind === "app" && strip.direct.length > 0
            text: "→ " + strip.direct
            elide: Text.ElideRight
            font: Kirigami.Theme.smallFont
            opacity: 0.7
            Layout.fillWidth: true
        }

        // Buses: which hardware output they play to.
        QQC2.ComboBox {
            id: outputBox
            visible: strip.kind === "bus"
            Layout.fillWidth: true
            // Long device names are elided rather than widening the strip.
            Layout.preferredWidth: 0
            textRole: "label"
            valueRole: "name"
            model: {
                const choices = [{ name: "", label: "No output" }].concat(strip.outputChoices)
                if (strip.output && !choices.some(c => c.name === strip.output))
                    choices.push({ name: strip.output, label: strip.outputLabel + " (disconnected)" })
                return choices
            }
            currentIndex: model.findIndex(c => c.name === strip.output)
            onActivated: strip.mixer.setBusOutput(strip.busName, currentValue)
            QQC2.ToolTip.text: "Plays to: " + displayText
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        }
    }
}
