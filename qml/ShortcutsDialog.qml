// Lists the global shortcuts and edits them.

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kquickcontrols as KQuickControls

Kirigami.Dialog {
    id: dialog

    required property var shortcuts
    required property var mixer

    title: "Shortcuts"
    preferredWidth: Kirigami.Units.gridUnit * 36
    padding: Kirigami.Units.largeSpacing
    standardButtons: Kirigami.Dialog.Close
    customFooterActions: [
        Kirigami.Action {
            text: "Add Shortcut…"
            icon.name: "list-add"
            enabled: dialog.shortcuts.available
            onTriggered: editor.openFor(null)
        }
    ]

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Kirigami.InlineMessage {
            visible: !dialog.shortcuts.available
            type: Kirigami.MessageType.Warning
            text: "Global shortcuts need KDE Plasma's shortcut service (kglobalaccel), which isn't running."
            Layout.fillWidth: true
        }
        QQC2.Label {
            text: "Shortcuts work everywhere, also while Hepcat sits in the tray. You can change their keys here or in "
                + "System Settings → Keyboard → Shortcuts → Hepcat."
            wrapMode: Text.WordWrap
            opacity: 0.7
            Layout.fillWidth: true
        }
        QQC2.Label {
            visible: dialog.shortcuts.list.length === 0
            text: "No shortcuts yet. Add one to mute a microphone, push to talk, switch effects, change volume, "
                + "send an app to a bus and more."
            wrapMode: Text.WordWrap
            Layout.fillWidth: true
        }

        Repeater {
            model: dialog.shortcuts.list
            delegate: RowLayout {
                id: row
                required property var modelData
                readonly property bool missing: modelData.target !== "hepcat" && !dialog.shortcuts.find(modelData.target)
                spacing: Kirigami.Units.smallSpacing
                Layout.fillWidth: true

                ColumnLayout {
                    spacing: 0
                    Layout.fillWidth: true
                    QQC2.Label {
                        text: dialog.shortcuts.describe(row.modelData)
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                    QQC2.Label {
                        visible: row.missing
                        text: "Not available right now"
                        font: Kirigami.Theme.smallFont
                        opacity: 0.7
                    }
                }
                QQC2.Label {
                    text: row.modelData.keys ? dialog.mixer.keysText(row.modelData.keys) : "No keys"
                    opacity: row.modelData.keys ? 1 : 0.6
                    font.family: "monospace"
                }
                QQC2.ToolButton {
                    icon.name: "document-edit"
                    display: QQC2.AbstractButton.IconOnly
                    text: "Edit"
                    QQC2.ToolTip.text: text
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    onClicked: editor.openFor(row.modelData)
                }
                QQC2.ToolButton {
                    icon.name: "edit-delete"
                    display: QQC2.AbstractButton.IconOnly
                    text: "Remove"
                    QQC2.ToolTip.text: text
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    onClicked: dialog.shortcuts.remove(row.modelData.id)
                }
            }
        }
    }

    Kirigami.Dialog {
        id: editor

        // The shortcut being edited, or null for a new one.
        property var editing: null
        property var targets: []
        readonly property var target: targets[targetBox.currentIndex] || null
        readonly property var actionChoices: target ? dialog.shortcuts.actions.filter(a => a.kinds.includes(target.kind)) : []
        readonly property var action: actionChoices[actionBox.currentIndex] || null
        readonly property string param: action ? (action.param || "") : ""
        property string keys: ""

        readonly property var choices: {
            const s = dialog.shortcuts.state
            if (param === "effect")
                return dialog.shortcuts.effects
            if (param === "bus")
                return s ? s.buses.map(b => ({ id: b.name, label: b.label })) : []
            if (param === "output")
                return s ? s.outputs.map(o => ({ id: o.name, label: o.label })) : []
            return []
        }

        function openFor(sc) {
            editing = sc
            targets = dialog.shortcuts.targets()
            if (sc && !targets.some(t => t.uid === sc.target))
                targets = targets.concat([{ uid: sc.target, kind: sc.targetKind || "app", label: sc.targetLabel + " (not available)" }])
            targetBox.currentIndex = sc ? Math.max(0, targets.findIndex(t => t.uid === sc.target)) : Math.min(1, targets.length - 1)
            actionBox.currentIndex = sc ? Math.max(0, actionChoices.findIndex(a => a.id === sc.action)) : 0
            if (sc && (param === "step" || param === "level"))
                amount.value = sc.param
            else
                amount.value = param === "level" ? 100 : 5
            choiceBox.currentIndex = sc ? Math.max(0, choices.findIndex(c => c.id === sc.param)) : 0
            keys = sc ? sc.keys : ""
            keyItem.keySequence = dialog.mixer.sequenceFromKeys(keys)
            open()
        }

        function accept() {
            const p = param === "step" || param === "level" ? amount.value
                    : param ? (choices[choiceBox.currentIndex] || {}).id : undefined
            const choice = choices[choiceBox.currentIndex]
            dialog.shortcuts.put({
                id: editing ? editing.id : "s" + Date.now(),
                keys: keys,
                target: target.uid,
                targetKind: target.kind,
                targetLabel: target.label.replace(/ \(not available\)$/, ""),
                action: action.id,
                param: p,
                paramLabel: param === "output" && choice ? choice.label : undefined,
            })
            close()
        }

        title: editing ? "Edit Shortcut" : "Add Shortcut"
        preferredWidth: Kirigami.Units.gridUnit * 28
        padding: Kirigami.Units.largeSpacing
        standardButtons: Kirigami.Dialog.Cancel
        customFooterActions: [
            Kirigami.Action {
                text: "Save"
                icon.name: "document-save"
                enabled: editor.target !== null && editor.action !== null && editor.keys.length > 0
                         && (editor.choices.length > 0 || editor.param === "" || editor.param === "step" || editor.param === "level")
                onTriggered: editor.accept()
            }
        ]

        Kirigami.FormLayout {
            QQC2.ComboBox {
                id: targetBox
                Kirigami.FormData.label: "Control:"
                model: editor.targets.map(t => t.kind === "hepcat" ? t.label : dialog.shortcuts.kindNames[t.kind] + ": " + t.label)
                Layout.preferredWidth: Kirigami.Units.gridUnit * 16
                onActivated: actionBox.currentIndex = 0
            }
            QQC2.ComboBox {
                id: actionBox
                Kirigami.FormData.label: "Action:"
                model: editor.actionChoices.map(a => a.label)
                Layout.preferredWidth: Kirigami.Units.gridUnit * 16
            }
            QQC2.SpinBox {
                id: amount
                visible: editor.param === "step" || editor.param === "level"
                Kirigami.FormData.label: editor.param === "step" ? "Step:" : "Volume:"
                from: editor.param === "step" ? 1 : 0
                to: 100
                textFromValue: v => v + "%"
                valueFromText: t => parseInt(t)
            }
            QQC2.ComboBox {
                id: choiceBox
                visible: editor.param === "effect" || editor.param === "bus" || editor.param === "output"
                Kirigami.FormData.label: ({ effect: "Effect:", bus: "Bus:", output: "Device:" })[editor.param] ?? ""
                model: editor.choices.map(c => c.label)
                Layout.preferredWidth: Kirigami.Units.gridUnit * 16
            }
            KQuickControls.KeySequenceItem {
                id: keyItem
                Kirigami.FormData.label: "Keys:"
                showCancelButton: true
                onKeySequenceModified: editor.keys = dialog.mixer.keysFromSequence(keySequence)
            }
            QQC2.Label {
                visible: editor.action !== null && (editor.action.id === "hold_unmute" || editor.action.id === "hold_mute")
                text: "Mutes or unmutes while the keys are held, and switches back when they are let go."
                wrapMode: Text.WordWrap
                opacity: 0.7
                font: Kirigami.Theme.smallFont
                Layout.maximumWidth: Kirigami.Units.gridUnit * 16
            }
        }
    }
}
