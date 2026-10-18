// One mixer channel: an app, microphone, bus or hardware output. Drawn as a
// tall fader strip, or as a small horizontal tile when `compact`.

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
    // App-only: whether it plays right now, and whether it was added by hand
    // to stay in the mixer while silent.
    property bool running: true
    property bool kept: false
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
        const strength = ["", " (light)", " (medium)", "", " (maximum)"][f.noise_strength ?? 3] ?? ""
        if (f.noise === "standard") parts.push("Noise suppression" + strength)
        if (f.noise === "voice") parts.push("Voice isolation" + strength)
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

    // A small horizontal tile instead of a tall fader strip.
    property bool compact: false
    property bool canMoveBack: false
    property bool canMoveForward: false
    // Strips follow each other top to bottom (tiles in columns).
    property bool movesVertically: false

    signal removeRequested()
    signal renameRequested()
    signal forgetRequested()
    signal effectsRequested()
    signal hideRequested()
    // -1 to move back (left or up), +1 to move forward.
    signal moveRequested(int delta)

    readonly property bool isSource: kind === "app" || kind === "mic"
    // The narrowest the strip gets with everything still usable.
    readonly property real minimumWidth: compact
        ? Kirigami.Units.gridUnit * 10
        : buttonRow.implicitWidth + leftPadding + rightPadding

    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: stripMenu.popup()
    }

    implicitWidth: compact ? Kirigami.Units.gridUnit * 15 : Math.max(Kirigami.Units.gridUnit * 8, minimumWidth)
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

    QQC2.Menu {
        id: stripMenu
        QQC2.MenuItem {
            text: "Hide"
            icon.name: "view-hidden"
            onTriggered: strip.hideRequested()
        }
        QQC2.MenuItem {
            text: strip.movesVertically ? "Move Up" : "Move Left"
            icon.name: strip.movesVertically ? "go-up" : "go-previous"
            enabled: strip.canMoveBack
            onTriggered: strip.moveRequested(-1)
        }
        QQC2.MenuItem {
            text: strip.movesVertically ? "Move Down" : "Move Right"
            icon.name: strip.movesVertically ? "go-down" : "go-next"
            enabled: strip.canMoveForward
            onTriggered: strip.moveRequested(1)
        }
        QQC2.MenuSeparator {
            visible: strip.kind === "bus" || strip.kept
            height: visible ? implicitHeight : 0
        }
        QQC2.MenuItem {
            visible: strip.kept
            height: visible ? implicitHeight : 0
            text: strip.running ? "Don't Keep When Silent" : "Remove from Mixer"
            icon.name: strip.running ? "window-unpin" : "list-remove"
            onTriggered: strip.forgetRequested()
        }
        QQC2.MenuItem {
            visible: strip.kind === "bus"
            height: visible ? implicitHeight : 0
            text: "Rename Bus…"
            icon.name: "edit-rename"
            onTriggered: strip.renameRequested()
        }
        QQC2.MenuItem {
            visible: strip.kind === "bus"
            height: visible ? implicitHeight : 0
            text: "Remove Bus…"
            icon.name: "edit-delete"
            onTriggered: strip.removeRequested()
        }
    }

    // Shown while effects are on; hover lists them, click edits them.
    component FxBadge: QQC2.AbstractButton {
        id: fxBadge
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
            // In a tile, the effects badge sits next to the name.
            FxBadge {
                visible: strip.fxEnabled && strip.compact
            }
            QQC2.ToolButton {
                icon.name: "overflow-menu"
                display: QQC2.AbstractButton.IconOnly
                text: "More"
                padding: 0
                Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium + Kirigami.Units.smallSpacing * 2
                Layout.preferredHeight: Layout.preferredWidth
                QQC2.ToolTip.text: (strip.kind === "bus" ? "Hide, move, rename or remove" : "Hide or move") + " (also on right-click)"
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                onClicked: stripMenu.visible ? stripMenu.close() : stripMenu.popup(this, 0, height)
            }
        }

        // In a fader strip this is laid out even when empty, so faders line
        // up, and holds the effects badge.
        RowLayout {
            visible: !strip.compact || detailLabel.text.length > 0
            spacing: Kirigami.Units.smallSpacing
            Layout.fillWidth: true

            QQC2.Label {
                id: detailLabel
                text: strip.kind === "bus" && !strip.online ? "Starting…"
                    : strip.detail !== strip.label ? strip.detail : ""
                elide: Text.ElideRight
                opacity: 0.7
                font: Kirigami.Theme.smallFont
                Layout.fillWidth: true
            }
            FxBadge {
                visible: strip.fxEnabled && !strip.compact
                // No taller than the line, so faders stay level across strips.
                Layout.maximumHeight: detailLabel.implicitHeight
            }
        }

        // Strip: fader, percentage, buttons, then routing, top to bottom.
        // Tile: buttons, fader and percentage in one row, routing under them.
        GridLayout {
            columns: strip.compact ? 3 : 1
            columnSpacing: Kirigami.Units.smallSpacing
            rowSpacing: Kirigami.Units.smallSpacing
            Layout.fillWidth: true
            Layout.fillHeight: !strip.compact

            Fader {
                id: fader
                Layout.row: 0
                Layout.column: strip.compact ? 1 : 0
                vertical: !strip.compact
                volume: strip.volume
                level: strip.level
                mute: strip.mute
                enabled: strip.online && strip.running
                Layout.fillHeight: !strip.compact
                Layout.fillWidth: strip.compact
                Layout.minimumHeight: strip.compact ? 0 : Kirigami.Units.gridUnit * 4
                Layout.alignment: Qt.AlignHCenter | Qt.AlignVCenter
                onMoved: value => strip.mixer.setVolume(strip.ids, value)
            }
            QQC2.Label {
                Layout.row: strip.compact ? 0 : 1
                Layout.column: strip.compact ? 2 : 0
                text: strip.running ? Math.round(fader.value * 100) + "%" : "–"
                font.features: { "tnum": 1 }
                horizontalAlignment: Text.AlignRight
                Layout.alignment: Qt.AlignHCenter | Qt.AlignVCenter
                // Room for "100%", so a tile's slider does not jump while dragging.
                Layout.minimumWidth: strip.compact ? digits.advanceWidth("100%") : 0
                FontMetrics { id: digits }
            }

            RowLayout {
                id: buttonRow
                Layout.row: strip.compact ? 0 : 2
                Layout.column: 0
                Layout.alignment: Qt.AlignHCenter | Qt.AlignVCenter
                spacing: 0
                enabled: strip.online

                QQC2.ToolButton {
                    enabled: strip.running
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
                id: routes
                Layout.row: 3
                Layout.column: 0
                Layout.columnSpan: strip.compact ? 3 : 1
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
                        // Two per row in a strip; as wide as needed in a tile.
                        width: strip.compact ? Math.min(implicitWidth, routes.width)
                                             : (routes.width - Kirigami.Units.smallSpacing) / 2
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

            // Buses: which hardware output they play to.
            QQC2.ComboBox {
                id: outputBox
                visible: strip.kind === "bus"
                Layout.row: 4
                Layout.column: 0
                Layout.columnSpan: strip.compact ? 3 : 1
                Layout.fillWidth: true
                // Long device names are elided rather than widening the strip.
                Layout.preferredWidth: 0
                Layout.alignment: Qt.AlignVCenter
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

            QQC2.Label {
                visible: strip.kind === "app" && strip.direct.length > 0
                Layout.row: 5
                Layout.column: 0
                Layout.columnSpan: strip.compact ? 3 : 1
                text: "→ " + strip.direct
                elide: Text.ElideRight
                font: Kirigami.Theme.smallFont
                opacity: 0.7
                Layout.fillWidth: true
            }
        }
    }
}
