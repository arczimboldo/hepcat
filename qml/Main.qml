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
    Component.onCompleted: backend.setMetersEnabled(visible)
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

    ListModel { id: micModel }
    ListModel { id: appModel }
    ListModel { id: busModel }
    ListModel { id: outputModel }
    // Plain array of { name, label } for the bus output pickers.
    property var outputChoices: []
    property bool loaded: false

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
        const s = JSON.parse(backend.stateJson)
        syncModel(micModel, s.mics, "key")
        syncModel(appModel, s.apps, "key")
        syncModel(busModel, s.buses, "name")
        syncModel(outputModel, s.outputs, "name")
        outputChoices = s.outputs.map(o => ({ name: o.name, label: o.label }))
        loaded = true
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

    pageStack.initialPage: Kirigami.Page {
        title: "Mixer"
        padding: Kirigami.Units.largeSpacing

        Kirigami.LoadingPlaceholder {
            anchors.centerIn: parent
            visible: !root.loaded
            text: "Connecting to PipeWire…"
        }

        QQC2.ScrollView {
            id: scroller
            anchors.fill: parent
            visible: root.loaded
            contentWidth: sections.implicitWidth
            contentHeight: availableHeight

            RowLayout {
                id: sections
                height: scroller.availableHeight
                spacing: Kirigami.Units.gridUnit

                Section {
                    title: "Microphones"
                    empty: micModel.count === 0
                    placeholder: "No input devices"
                    Layout.fillHeight: true

                    Repeater {
                        model: micModel
                        delegate: Strip {
                            required property var model
                            kind: "mic"
                            mixer: backend
                            busList: busModel
                            key: model.key
                            micName: model.name
                            label: model.label
                            icon: model.icon
                            ids: model.ids
                            volume: model.volume
                            mute: model.mute
                            level: root.levelOf(model.meterIds)
                            buses: model.buses
                            isDefault: model.isDefault
                            fx: model.fx
                            fxActive: model.fxActive
                            fxDevice: model.fxDevice
                            Layout.fillHeight: true
                            onEffectsRequested: effectsDialog.openFor(model.key, "mic", model.label, model.fx)
                        }
                    }
                }

                Kirigami.Separator { Layout.fillHeight: true }

                Section {
                    title: "Applications"
                    empty: appModel.count === 0
                    placeholder: "Nothing is playing right now"
                    Layout.fillHeight: true

                    Repeater {
                        model: appModel
                        delegate: Strip {
                            required property var model
                            kind: "app"
                            mixer: backend
                            busList: busModel
                            key: model.key
                            label: model.label
                            detail: model.detail
                            icon: model.icon
                            ids: model.ids
                            volume: model.volume
                            mute: model.mute
                            level: root.levelOf(model.meterIds)
                            buses: model.buses
                            direct: model.direct
                            fx: model.fx
                            fxActive: model.fxActive
                            Layout.fillHeight: true
                            onEffectsRequested: effectsDialog.openFor(model.key, "app", model.label, model.fx)
                        }
                    }
                }

                Kirigami.Separator { Layout.fillHeight: true }

                Section {
                    title: "Buses"
                    Layout.fillHeight: true

                    Repeater {
                        model: busModel
                        delegate: Strip {
                            required property var model
                            kind: "bus"
                            mixer: backend
                            outputChoices: root.outputChoices
                            key: model.name
                            busName: model.name
                            nodeName: model.nodeName
                            label: model.label
                            ids: model.ids
                            volume: model.volume
                            mute: model.mute
                            level: root.levelOf(model.meterIds)
                            output: model.output
                            outputLabel: model.outputLabel
                            online: model.online
                            micEnabled: model.mic
                            isDefault: model.isDefault
                            fx: model.fx
                            fxActive: model.fxActive
                            Layout.fillHeight: true
                            onEffectsRequested: effectsDialog.openFor("bus:" + model.name, "bus", model.label, model.fx)
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
                        text: "Add Bus"
                        hoverEnabled: true
                        Layout.fillHeight: true
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 5
                        QQC2.ToolTip.text: "Create a virtual output that apps can play into"
                        QQC2.ToolTip.visible: hovered
                        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                        onClicked: addBusDialog.open()

                        contentItem: ColumnLayout {
                            spacing: Kirigami.Units.smallSpacing
                            Item { Layout.fillHeight: true }
                            Kirigami.Icon {
                                source: "list-add"
                                color: addBusTile.hovered ? Kirigami.Theme.highlightColor : Kirigami.Theme.textColor
                                Layout.alignment: Qt.AlignHCenter
                                Layout.preferredWidth: Kirigami.Units.iconSizes.medium
                                Layout.preferredHeight: Kirigami.Units.iconSizes.medium
                            }
                            QQC2.Label {
                                text: addBusTile.text
                                Layout.alignment: Qt.AlignHCenter
                            }
                            Item { Layout.fillHeight: true }
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

                Kirigami.Separator { Layout.fillHeight: true }

                Section {
                    title: "Outputs"
                    empty: outputModel.count === 0
                    placeholder: "No output devices"
                    Layout.fillHeight: true

                    Repeater {
                        model: outputModel
                        delegate: Strip {
                            required property var model
                            kind: "output"
                            mixer: backend
                            key: model.name
                            label: model.label
                            detail: model.detail
                            icon: model.icon
                            ids: model.ids
                            volume: model.volume
                            mute: model.mute
                            level: root.levelOf(model.ids)
                            isDefault: model.isDefault
                            Layout.fillHeight: true
                        }
                    }
                }
            }
        }
    }
}
