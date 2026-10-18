import QtCore
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.hepcat

Kirigami.ApplicationWindow {
    id: root

    title: "Hepcat"

    // Set by the launcher for `--minimized` (autostart): live in the tray.
    property bool startHidden: false
    visible: !startHidden
    onVisibleChanged: backend.setMetersEnabled(visible)
    Component.onCompleted: {
        backend.setMetersEnabled(visible)
        if (backend.stateJson)
            sync()
        if (backend.levelsJson)
            levels = JSON.parse(backend.levelsJson)
    }
    width: Kirigami.Units.gridUnit * 64
    height: Kirigami.Units.gridUnit * 34
    minimumWidth: Kirigami.Units.gridUnit * 24
    minimumHeight: Kirigami.Units.gridUnit * 26

    Mixer {
        id: backend
        onStateJsonChanged: root.sync()
        onLevelsJsonChanged: root.levels = JSON.parse(levelsJson)
    }

    // Node id -> meter level, refreshed ~25 times a second.
    property var levels: ({})
    function levelOf(ids) {
        return ids.split(",").reduce((m, id) => Math.max(m, levels[id] || 0), 0)
    }

    // Layout preferences, kept next to config.toml.
    Settings {
        id: ui
        location: StandardPaths.writableLocation(StandardPaths.GenericConfigLocation) + "/hepcat/layout.conf"
        // "row": everything in one row that scrolls sideways; "wrap": strips
        // wrap into rows and the page scrolls down.
        property string mode: "row"
        // "faders" (tall strips) or "compact" (small horizontal tiles).
        property string style: "faders"
        property string sectionOrder: "[]"
        // JSON: section ids that are hidden.
        property string hiddenSections: "[]"
        // JSON: strip uid -> { label, section } of hidden strips.
        property string hidden: "{}"
        // JSON: section id -> strip uids in the order the user put them.
        property string order: "{}"
        // JSON: style -> section id -> { w, h } in grid units.
        property string sizes: "{}"
    }

    readonly property bool wrap: ui.mode === "wrap"
    readonly property bool compact: ui.style === "compact"
    readonly property var hiddenStrips: parse(ui.hidden, {})
    readonly property var hiddenSectionIds: parse(ui.hiddenSections, [])
    readonly property var stripOrder: parse(ui.order, {})
    readonly property var stripSizes: parse(ui.sizes, {})

    readonly property var sectionInfo: ({
        mics: { title: "Microphones", kind: "mic", placeholder: "No input devices", model: micModel },
        apps: { title: "Applications", kind: "app", placeholder: "Nothing is playing right now", model: appModel },
        buses: { title: "Buses", kind: "bus", placeholder: "", model: busModel },
        outputs: { title: "Outputs", kind: "output", placeholder: "No output devices", model: outputModel },
    })
    readonly property var defaultSectionOrder: ["mics", "apps", "buses", "outputs"]
    // Every section, in the user's order (new ones at the end).
    readonly property var sectionOrder: {
        const saved = parse(ui.sectionOrder, []).filter(id => defaultSectionOrder.includes(id))
        return saved.concat(defaultSectionOrder.filter(id => !saved.includes(id)))
    }
    readonly property var shownSections: sectionOrder.filter(id => !hiddenSectionIds.includes(id))

    function parse(text, fallback) {
        try {
            return JSON.parse(text) ?? fallback
        } catch (e) {
            return fallback
        }
    }

    function hideStrip(section, uid, label) {
        const h = Object.assign({}, hiddenStrips)
        h[uid] = { label: label, section: section }
        ui.hidden = JSON.stringify(h)
        refresh()
    }
    function showStrip(uid) {
        const h = Object.assign({}, hiddenStrips)
        delete h[uid]
        ui.hidden = JSON.stringify(h)
        refresh()
    }
    function setSectionHidden(id, hide) {
        const h = hiddenSectionIds.filter(s => s !== id)
        if (hide)
            h.push(id)
        ui.hiddenSections = JSON.stringify(h)
    }
    function showEverything() {
        ui.hidden = "{}"
        ui.hiddenSections = "[]"
        refresh()
    }
    function moveSection(id, delta) {
        // Move among the shown sections; hidden ones keep their place.
        const shown = shownSections
        const to = shown.indexOf(id) + delta
        if (to < 0 || to >= shown.length)
            return
        const order = sectionOrder.slice()
        const a = order.indexOf(id), b = order.indexOf(shown[to])
        order[a] = shown[to]
        order[b] = id
        ui.sectionOrder = JSON.stringify(order)
    }
    function moveStrip(section, uid, delta) {
        const model = sectionInfo[section].model
        const uids = []
        for (let i = 0; i < model.count; i++)
            uids.push(model.get(i).uid)
        const from = uids.indexOf(uid), to = from + delta
        if (from < 0 || to < 0 || to >= uids.length)
            return
        uids[from] = uids[to]
        uids[to] = uid
        // Keep the places of strips that are hidden or gone for now.
        const o = Object.assign({}, stripOrder)
        o[section] = uids.concat((o[section] || []).filter(u => !uids.includes(u)))
        ui.order = JSON.stringify(o)
        refresh()
    }
    // Width and height in pixels, 0 for the default.
    function sizeOf(section) {
        const s = (stripSizes[ui.style] || {})[section] || {}
        return { w: (s.w || 0) * Kirigami.Units.gridUnit, h: (s.h || 0) * Kirigami.Units.gridUnit }
    }
    function resize(section, w, h) {
        const all = Object.assign({}, stripSizes)
        const style = Object.assign({}, all[ui.style] || {})
        const s = Object.assign({}, style[section] || {})
        if (w >= 0) s.w = w > 0 ? w / Kirigami.Units.gridUnit : undefined
        if (h >= 0) s.h = h > 0 ? h / Kirigami.Units.gridUnit : undefined
        style[section] = s
        all[ui.style] = style
        ui.sizes = JSON.stringify(all)
    }
    function resetLayout() {
        ui.mode = "row"
        ui.style = "faders"
        ui.sectionOrder = "[]"
        ui.order = "{}"
        ui.sizes = "{}"
        showEverything()
    }

    // What is hidden, for the dialog that brings it back: sections first.
    readonly property var hiddenList: {
        const list = hiddenSectionIds.filter(id => sectionInfo[id]).map(id => ({
            uid: "section:" + id, label: sectionInfo[id].title, detail: "Whole section",
        }))
        for (const uid in hiddenStrips) {
            const h = hiddenStrips[uid]
            list.push({
                uid: uid, label: h.label,
                detail: (sectionInfo[h.section] || { title: "" }).title
                    + (presentUids.includes(uid) ? "" : " · not running now"),
            })
        }
        return list
    }
    // Uids of every strip that exists right now, hidden or not.
    property var presentUids: []
    // Hidden strips that exist right now, by section.
    property var hiddenCounts: ({})

    ListModel { id: micModel }
    ListModel { id: appModel }
    ListModel { id: busModel }
    ListModel { id: outputModel }
    // Every bus, also hidden ones, for the routing buttons on sources.
    ListModel { id: allBusModel }
    // Plain array of { name, label } for the bus output pickers.
    property var outputChoices: []
    property bool loaded: false
    property var lastState: null

    // ListModels are updated in place (rather than replaced) so strips are not
    // recreated while the user is dragging a slider.
    function syncModel(model, items, keyField) {
        const flat = items.map(item => {
            const out = {}
            for (const k in item) {
                const v = item[k]
                out[k] = !Array.isArray(v) ? v : (k === "ids" || k === "meterIds" ? v.join(",") : "|" + v.join("|") + "|")
            }
            return out
        })
        const keys = flat.map(f => f[keyField])
        for (let i = model.count - 1; i >= 0; i--) {
            if (keys.indexOf(model.get(i)[keyField]) < 0)
                model.remove(i)
        }
        flat.forEach((f, idx) => {
            let cur = -1
            for (let j = 0; j < model.count; j++) {
                if (model.get(j)[keyField] === f[keyField]) { cur = j; break }
            }
            if (cur < 0) {
                model.insert(idx, f)
            } else {
                if (cur !== idx)
                    model.move(cur, idx, 1)
                model.set(idx, f)
            }
        })
    }

    function sync() {
        lastState = JSON.parse(backend.stateJson)
        refresh()
        loaded = true
    }

    // Apply hiding and ordering to the last state from the backend.
    function refresh() {
        const s = lastState
        if (!s)
            return
        const items = {
            mics: s.mics.map(i => Object.assign({ uid: i.key }, i)),
            apps: s.apps.map(i => Object.assign({ uid: i.key }, i)),
            buses: s.buses.map(i => Object.assign({ uid: "bus:" + i.name, key: "bus:" + i.name }, i)),
            outputs: s.outputs.map(i => Object.assign({ uid: "out:" + i.name, key: i.name, meterIds: i.ids }, i)),
        }
        const counts = {}
        const present = []
        for (const id in items) {
            const order = stripOrder[id] || []
            const rank = (uid, natural) => {
                const i = order.indexOf(uid)
                return i >= 0 ? i : order.length + natural
            }
            const ranked = items[id].map((item, natural) => ({ item: item, rank: rank(item.uid, natural) }))
            ranked.sort((a, b) => a.rank - b.rank)
            const shown = ranked.map(r => r.item).filter(item => !(item.uid in hiddenStrips))
            counts[id] = items[id].length - shown.length
            items[id].forEach(item => present.push(item.uid))
            syncModel(sectionInfo[id].model, shown, "uid")
        }
        syncModel(allBusModel, s.buses, "name")
        outputChoices = s.outputs.map(o => ({ name: o.name, label: o.label }))
        hiddenCounts = counts
        presentUids = present
    }

    Kirigami.PromptDialog {
        id: addBusDialog
        title: "Add Bus"
        standardButtons: Kirigami.Dialog.Ok | Kirigami.Dialog.Cancel
        onOpened: { busNameField.text = ""; busNameField.forceActiveFocus() }
        onAccepted: backend.addBus(busNameField.text)

        ColumnLayout {
            QQC2.Label {
                text: "A bus is a virtual output that apps can play into. Mix sources into it and send it to any device."
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
            QQC2.TextField {
                id: busNameField
                placeholderText: "Name, e.g. Music or Stream"
                Layout.fillWidth: true
                onAccepted: addBusDialog.accept()
            }
        }
    }

    EffectsDialog {
        id: effectsDialog
        mixer: backend
    }

    Kirigami.PromptDialog {
        id: removeBusDialog
        property string busName
        property string busLabel
        title: "Remove “" + busLabel + "”?"
        subtitle: "Apps playing into this bus will move back to the default output."
        standardButtons: Kirigami.Dialog.Ok | Kirigami.Dialog.Cancel
        onAccepted: backend.removeBus(busName)
    }

    Kirigami.Dialog {
        id: hiddenDialog
        title: "Hidden"
        preferredWidth: Kirigami.Units.gridUnit * 24
        padding: Kirigami.Units.largeSpacing
        standardButtons: Kirigami.Dialog.Close
        customFooterActions: [
            Kirigami.Action {
                text: "Show All"
                icon.name: "view-visible"
                enabled: root.hiddenList.length > 0
                onTriggered: {
                    root.showEverything()
                    hiddenDialog.close()
                }
            }
        ]
        ColumnLayout {
            spacing: Kirigami.Units.smallSpacing

            QQC2.Label {
                visible: root.hiddenList.length === 0
                text: "Nothing is hidden. Hide a strip or section from its ⋯ menu or by right-clicking it."
                wrapMode: Text.WordWrap
                opacity: 0.7
                Layout.fillWidth: true
            }
            Repeater {
                model: root.hiddenList
                delegate: RowLayout {
                    required property var modelData
                    spacing: Kirigami.Units.largeSpacing
                    Layout.fillWidth: true

                    ColumnLayout {
                        spacing: 0
                        Layout.fillWidth: true
                        QQC2.Label {
                            text: modelData.label
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }
                        QQC2.Label {
                            text: modelData.detail
                            font: Kirigami.Theme.smallFont
                            opacity: 0.7
                            Layout.fillWidth: true
                        }
                    }
                    QQC2.Button {
                        text: "Show"
                        icon.name: "view-visible"
                        onClicked: modelData.uid.startsWith("section:")
                            ? root.setSectionHidden(modelData.uid.slice(8), false)
                            : root.showStrip(modelData.uid)
                    }
                }
            }
        }
    }

    pageStack.initialPage: Kirigami.Page {
        title: "Mixer"
        padding: Kirigami.Units.largeSpacing

        actions: [
            Kirigami.Action {
                text: root.hiddenList.length + " Hidden"
                icon.name: "view-hidden"
                visible: root.hiddenList.length > 0
                tooltip: "Show strips and sections you hid"
                onTriggered: hiddenDialog.open()
            },
            Kirigami.Action {
                text: "View"
                icon.name: "view-list-details"
                tooltip: "Layout, strip size and which sections are shown"

                Kirigami.Action {
                    text: "One Row (scroll sideways)"
                    icon.name: "view-split-left-right"
                    checkable: true
                    checked: !root.wrap
                    onTriggered: {
                        ui.mode = "row"
                        checked = Qt.binding(() => !root.wrap)
                    }
                }
                Kirigami.Action {
                    text: "Wrap into Rows (scroll down)"
                    icon.name: "view-grid"
                    checkable: true
                    checked: root.wrap
                    onTriggered: {
                        ui.mode = "wrap"
                        checked = Qt.binding(() => root.wrap)
                    }
                }
                Kirigami.Action { separator: true }
                Kirigami.Action {
                    text: "Fader Strips"
                    icon.name: "view-media-equalizer"
                    checkable: true
                    checked: !root.compact
                    onTriggered: {
                        ui.style = "faders"
                        checked = Qt.binding(() => !root.compact)
                    }
                }
                Kirigami.Action {
                    text: "Compact Tiles"
                    icon.name: "view-list-text"
                    checkable: true
                    checked: root.compact
                    onTriggered: {
                        ui.style = "compact"
                        checked = Qt.binding(() => root.compact)
                    }
                }
                Kirigami.Action { separator: true }
                Kirigami.Action {
                    text: "Microphones"
                    checkable: true
                    checked: !root.hiddenSectionIds.includes("mics")
                    onTriggered: {
                        root.setSectionHidden("mics", !checked)
                        checked = Qt.binding(() => !root.hiddenSectionIds.includes("mics"))
                    }
                }
                Kirigami.Action {
                    text: "Applications"
                    checkable: true
                    checked: !root.hiddenSectionIds.includes("apps")
                    onTriggered: {
                        root.setSectionHidden("apps", !checked)
                        checked = Qt.binding(() => !root.hiddenSectionIds.includes("apps"))
                    }
                }
                Kirigami.Action {
                    text: "Buses"
                    checkable: true
                    checked: !root.hiddenSectionIds.includes("buses")
                    onTriggered: {
                        root.setSectionHidden("buses", !checked)
                        checked = Qt.binding(() => !root.hiddenSectionIds.includes("buses"))
                    }
                }
                Kirigami.Action {
                    text: "Outputs"
                    checkable: true
                    checked: !root.hiddenSectionIds.includes("outputs")
                    onTriggered: {
                        root.setSectionHidden("outputs", !checked)
                        checked = Qt.binding(() => !root.hiddenSectionIds.includes("outputs"))
                    }
                }
                Kirigami.Action { separator: true }
                Kirigami.Action {
                    text: "Hidden Strips…"
                    icon.name: "view-hidden"
                    onTriggered: hiddenDialog.open()
                }
                Kirigami.Action {
                    text: "Reset Layout"
                    icon.name: "edit-reset"
                    tooltip: "Show everything again, in the original order, layout and sizes"
                    onTriggered: root.resetLayout()
                }
            }
        ]

        Kirigami.LoadingPlaceholder {
            anchors.centerIn: parent
            visible: !root.loaded
            text: "Connecting to PipeWire…"
        }

        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: parent.width - Kirigami.Units.gridUnit * 4
            visible: root.loaded && root.shownSections.length === 0
            icon.name: "view-hidden"
            text: "Every section is hidden"
            helpfulAction: Kirigami.Action {
                text: "Show All"
                icon.name: "view-visible"
                onTriggered: root.showEverything()
            }
        }

        QQC2.ScrollView {
            id: scroller
            anchors.fill: parent
            visible: root.loaded
            contentWidth: root.wrap ? availableWidth : sections.implicitWidth
            contentHeight: root.wrap ? sections.implicitHeight : availableHeight

            GridLayout {
                id: sections
                width: root.wrap ? scroller.availableWidth : implicitWidth
                height: root.wrap ? implicitHeight : scroller.availableHeight
                // One row of sections, or one under the other.
                flow: root.wrap ? GridLayout.TopToBottom : GridLayout.LeftToRight
                columnSpacing: Kirigami.Units.gridUnit
                rowSpacing: Kirigami.Units.gridUnit

                Repeater {
                    model: root.shownSections
                    delegate: GridLayout {
                        id: sectionCell
                        required property string modelData
                        required property int index
                        readonly property var info: root.sectionInfo[modelData]
                        readonly property var size: root.sizeOf(modelData)

                        flow: root.wrap ? GridLayout.TopToBottom : GridLayout.LeftToRight
                        columnSpacing: Kirigami.Units.gridUnit
                        rowSpacing: Kirigami.Units.gridUnit
                        Layout.fillWidth: root.wrap
                        Layout.fillHeight: !root.wrap

                        Kirigami.Separator {
                            visible: sectionCell.index > 0
                            Layout.fillHeight: !root.wrap
                            Layout.fillWidth: root.wrap
                        }

                        Section {
                            id: section
                            title: sectionCell.info.title
                            placeholder: sectionCell.info.placeholder
                            empty: sectionCell.info.model.count === 0 && sectionCell.modelData !== "buses"
                            hiddenCount: root.hiddenCounts[sectionCell.modelData] || 0
                            wrap: root.wrap
                            compact: root.compact
                            customWidth: sectionCell.size.w
                            customHeight: sectionCell.size.h
                            canMoveBack: sectionCell.index > 0
                            canMoveForward: sectionCell.index < root.shownSections.length - 1
                            Layout.fillWidth: root.wrap
                            Layout.fillHeight: !root.wrap
                            onHideRequested: root.setSectionHidden(sectionCell.modelData, true)
                            onMoveRequested: delta => root.moveSection(sectionCell.modelData, delta)
                            onShowHiddenRequested: hiddenDialog.open()

                            Repeater {
                                model: sectionCell.info.model
                                delegate: Strip {
                                    required property var model
                                    required property int index
                                    kind: sectionCell.info.kind
                                    mixer: backend
                                    busList: allBusModel
                                    outputChoices: root.outputChoices
                                    key: model.key
                                    label: model.label
                                    detail: model.detail ?? ""
                                    icon: model.icon ?? ""
                                    ids: model.ids
                                    volume: model.volume
                                    mute: model.mute
                                    level: root.levelOf(model.meterIds ?? "")
                                    buses: model.buses ?? ""
                                    direct: model.direct ?? ""
                                    isDefault: model.isDefault ?? false
                                    micName: kind === "mic" ? model.name : ""
                                    fxDevice: model.fxDevice ?? ""
                                    fx: model.fx ?? ""
                                    fxActive: model.fxActive ?? false
                                    busName: kind === "bus" ? model.name : ""
                                    nodeName: model.nodeName ?? ""
                                    output: model.output ?? ""
                                    outputLabel: model.outputLabel ?? ""
                                    online: model.online ?? true
                                    micEnabled: model.mic ?? false

                                    compact: root.compact
                                    canMoveBack: index > 0
                                    canMoveForward: index < sectionCell.info.model.count - 1
                                    heightResizable: root.wrap && !root.compact
                                    Layout.preferredWidth: Math.max(minimumWidth, section.stripWidth)
                                    Layout.fillHeight: !root.wrap && !root.compact
                                    Layout.preferredHeight: root.wrap && !root.compact ? section.stripHeight : -1
                                    Layout.alignment: Qt.AlignTop

                                    onEffectsRequested: effectsDialog.openFor(model.key, kind, model.label, model.fx)
                                    onHideRequested: root.hideStrip(sectionCell.modelData, model.uid, model.label)
                                    onMoveRequested: delta => root.moveStrip(sectionCell.modelData, model.uid, delta)
                                    onResizeRequested: (w, h) => root.resize(sectionCell.modelData, w, h)
                                    onRemoveRequested: {
                                        removeBusDialog.busName = model.name
                                        removeBusDialog.busLabel = model.label
                                        removeBusDialog.open()
                                    }
                                }
                            }

                            // Sits where the new bus will appear.
                            QQC2.AbstractButton {
                                id: addBusTile
                                visible: sectionCell.modelData === "buses"
                                text: "Add Bus"
                                hoverEnabled: true
                                Layout.fillHeight: !root.wrap && !root.compact
                                Layout.preferredWidth: root.compact ? section.stripWidth : Kirigami.Units.gridUnit * 5
                                Layout.preferredHeight: root.compact ? Math.max(implicitHeight, section.tileHeight)
                                                      : root.wrap ? section.stripHeight : -1
                                Layout.alignment: Qt.AlignTop
                                QQC2.ToolTip.text: "Create a virtual output that apps can play into"
                                QQC2.ToolTip.visible: hovered
                                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                                onClicked: addBusDialog.open()

                                contentItem: Item {
                                    implicitWidth: addBusContent.implicitWidth
                                    implicitHeight: addBusContent.implicitHeight

                                    // Icon over the label in a strip, beside it in a tile.
                                    GridLayout {
                                        id: addBusContent
                                        anchors.centerIn: parent
                                        columns: root.compact ? 2 : 1
                                        columnSpacing: Kirigami.Units.smallSpacing
                                        rowSpacing: Kirigami.Units.smallSpacing

                                        Kirigami.Icon {
                                            source: "list-add"
                                            color: addBusTile.hovered ? Kirigami.Theme.highlightColor : Kirigami.Theme.textColor
                                            Layout.alignment: Qt.AlignCenter
                                            Layout.preferredWidth: Kirigami.Units.iconSizes.medium
                                            Layout.preferredHeight: Kirigami.Units.iconSizes.medium
                                        }
                                        QQC2.Label {
                                            text: addBusTile.text
                                            Layout.alignment: Qt.AlignCenter
                                        }
                                    }
                                }

                                background: Rectangle {
                                    radius: Kirigami.Units.cornerRadius
                                    color: addBusTile.pressed ? Qt.alpha(Kirigami.Theme.highlightColor, 0.2)
                                         : addBusTile.hovered ? Qt.alpha(Kirigami.Theme.highlightColor, 0.1) : "transparent"
                                    border.width: 1
                                    border.color: addBusTile.hovered || addBusTile.visualFocus
                                        ? Kirigami.Theme.highlightColor
                                        : Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.2)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
