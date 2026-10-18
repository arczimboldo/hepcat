// Keep an app in the mixer while it plays nothing: pick one heard before, or
// type its name.

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Kirigami.Dialog {
    id: dialog

    required property var mixer
    // [{ key, label, icon }] of apps heard before that are not in the mixer.
    property var knownApps: []

    readonly property string typed: nameField.text.trim()
    readonly property var matches: {
        const q = typed.toLowerCase()
        return q ? knownApps.filter(a => a.label.toLowerCase().includes(q) || a.key.toLowerCase().includes(q)) : knownApps
    }

    function add(name) {
        mixer.addApp(name)
        close()
    }

    title: "Add App"
    preferredWidth: Kirigami.Units.gridUnit * 24
    padding: Kirigami.Units.largeSpacing
    standardButtons: Kirigami.Dialog.Cancel
    customFooterActions: [
        Kirigami.Action {
            text: dialog.typed ? "Add “" + dialog.typed + "”" : "Add"
            icon.name: "list-add"
            enabled: dialog.typed.length > 0
            onTriggered: dialog.add(dialog.typed)
        }
    ]
    onOpened: {
        nameField.text = ""
        nameField.forceActiveFocus()
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        QQC2.Label {
            text: "The app stays in the mixer while it is silent, so you can set up its buses and effects "
                + "before it plays. Handy for games that only make sound while they have focus."
            wrapMode: Text.WordWrap
            Layout.fillWidth: true
            // Keep the long text from stretching the dialog.
            Layout.preferredWidth: Kirigami.Units.gridUnit * 22
        }
        Kirigami.SearchField {
            id: nameField
            placeholderText: "Search apps heard before, or type a name"
            Layout.fillWidth: true
            onAccepted: {
                if (dialog.matches.length === 1)
                    dialog.add(dialog.matches[0].key)
                else if (dialog.typed)
                    dialog.add(dialog.typed)
            }
        }
        QQC2.ScrollView {
            visible: dialog.matches.length > 0
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(list.contentHeight, Kirigami.Units.gridUnit * 14)

            ListView {
                id: list
                clip: true
                model: dialog.matches
                delegate: QQC2.ItemDelegate {
                    required property var modelData
                    width: ListView.view.width
                    text: modelData.label
                    icon.name: modelData.icon || "applications-multimedia"
                    onClicked: dialog.add(modelData.key)
                }
            }
        }
        QQC2.Label {
            visible: dialog.matches.length === 0
            text: dialog.knownApps.length === 0
                ? "Apps you play show up here afterwards. You can also type the name an app gives its audio, the one shown on its strip."
                : "No app heard before matches. Add it by that name anyway, as long as it is the name on its strip while it plays."
            wrapMode: Text.WordWrap
            opacity: 0.7
            font: Kirigami.Theme.smallFont
            Layout.fillWidth: true
            Layout.preferredWidth: Kirigami.Units.gridUnit * 22
        }
    }
}
