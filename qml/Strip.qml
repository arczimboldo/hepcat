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
    // Show the grip on the bottom edge (for strips with a set height).
    property bool heightResizable: false

    signal removeRequested()
    signal effectsRequested()
    signal hideRequested()
    // -1 to move left, +1 to move right.
    signal moveRequested(int delta)
    // New size in pixels: 0 resets that dimension, -1 leaves it alone.
    signal resizeRequested(real width, real height)

    readonly property bool isSource: kind === "app" || kind === "mic"
    // The narrowest and shortest the strip gets with everything still usable.
    readonly property real minimumWidth: compact
        ? Kirigami.Units.gridUnit * 10
        : buttonRow.implicitWidth + leftPadding + rightPadding
    readonly property real minimumHeight: Kirigami.Units.gridUnit * 10

    HoverHandler { id: stripHover }
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: stripMenu.popup()
    }

    implicitWidth: compact ? Kirigami.Units.gridUnit * 15 : Math.max(Kirigami.Units.gridUnit * 7, minimumWidth)
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
            text: "Move Left"
            icon.name: "go-previous"
            enabled: strip.canMoveBack
            onTriggered: strip.moveRequested(-1)
        }
        QQC2.MenuItem {
            text: "Move Right"
            icon.name: "go-next"
            enabled: strip.canMoveForward
            onTriggered: strip.moveRequested(1)
        }
        QQC2.MenuSeparator {
            visible: strip.kind === "bus"
            height: visible ? implicitHeight : 0
        }
        QQC2.MenuItem {
            visible: strip.kind === "bus"
            height: visible ? implicitHeight : 0
            text: "Remove Bus…"
            icon.name: "edit-delete"
            onTriggered: strip.removeRequested()
        }
    }

    // A drag handle on the right or bottom edge; double-click resets.
    component Grip: MouseArea {
        id: grip
        property bool horizontal: true
        property real startPos
        property real startSize
        z: 1
        hoverEnabled: true
        cursorShape: horizontal ? Qt.SizeHorCursor : Qt.SizeVerCursor
        onPressed: mouse => {
            const p = mapToGlobal(mouse.x, mouse.y)
            startPos = horizontal ? p.x : p.y
            startSize = horizontal ? strip.width : strip.height
        }
        onPositionChanged: mouse => {
            if (!pressed)
                return
            const p = mapToGlobal(mouse.x, mouse.y)
            const size = startSize + (horizontal ? p.x : p.y) - startPos
            if (horizontal)
                strip.resizeRequested(Math.max(strip.minimumWidth, size), -1)
            else
                strip.resizeRequested(-1, Math.max(strip.minimumHeight, size))
        }
        onDoubleClicked: horizontal ? strip.resizeRequested(0, -1) : strip.resizeRequested(-1, 0)

        Rectangle {
            anchors.centerIn: parent
            width: grip.horizontal ? 2 : parent.width - Kirigami.Units.cornerRadius * 2
            height: grip.horizontal ? parent.height - Kirigami.Units.cornerRadius * 2 : 2
            color: Kirigami.Theme.highlightColor
            visible: grip.containsMouse || grip.pressed
        }
        QQC2.ToolTip.text: "Drag to resize all " + ({ app: "apps", mic: "inputs", bus: "buses", output: "outputs" })[strip.kind]
            + "; double-click to reset"
        QQC2.ToolTip.visible: containsMouse && !pressed
        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay * 2
    }
    Grip {
        anchors { top: parent.top; bottom: parent.bottom; right: parent.right; rightMargin: -width / 2 }
        width: Kirigami.Units.smallSpacing * 2
    }
    Grip {
        visible: strip.heightResizable
        horizontal: false
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom; bottomMargin: -height / 2 }
        height: Kirigami.Units.smallSpacing * 2
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
            // Only takes room while the pointer is over the strip.
            QQC2.ToolButton {
                visible: stripHover.hovered || stripMenu.visible
                icon.name: "overflow-menu"
                display: QQC2.AbstractButton.IconOnly
                text: "More"
                padding: 0
                Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium + Kirigami.Units.smallSpacing * 2
                Layout.preferredHeight: Layout.preferredWidth
                QQC2.ToolTip.text: (strip.kind === "bus" ? "Hide, move or remove" : "Hide or move") + " (also on right-click)"
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                onClicked: stripMenu.visible ? stripMenu.close() : stripMenu.popup(this, 0, height)
            }
        }

        // In a fader strip this is laid out even when empty, so faders line up.
        QQC2.Label {
            text: strip.kind === "bus" && !strip.online ? "Starting…"
                : strip.detail !== strip.label ? strip.detail : ""
            visible: !strip.compact || text.length > 0
            elide: Text.ElideRight
            opacity: 0.7
            font: Kirigami.Theme.smallFont
            Layout.fillWidth: true
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
                enabled: strip.online
                Layout.fillHeight: !strip.compact
                Layout.fillWidth: strip.compact
                Layout.minimumHeight: strip.compact ? 0 : Kirigami.Units.gridUnit * 4
                Layout.alignment: Qt.AlignHCenter | Qt.AlignVCenter
                onMoved: value => strip.mixer.setVolume(strip.ids, value)
            }
            QQC2.Label {
                Layout.row: strip.compact ? 0 : 1
                Layout.column: strip.compact ? 2 : 0
                text: Math.round(fader.value * 100) + "%"
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
