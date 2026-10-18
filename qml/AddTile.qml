// A dashed-looking "+" tile that sits where a new strip will appear, e.g.
// "Add Bus" or "Add App".

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

QQC2.AbstractButton {
    id: tile

    property string tooltip
    // Icon over the label, or beside it in a compact tile.
    property bool compact: false

    hoverEnabled: true
    QQC2.ToolTip.text: tooltip
    QQC2.ToolTip.visible: hovered && tooltip.length > 0
    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay

    contentItem: Item {
        implicitWidth: content.implicitWidth
        implicitHeight: content.implicitHeight

        GridLayout {
            id: content
            anchors.centerIn: parent
            columns: tile.compact ? 2 : 1
            columnSpacing: Kirigami.Units.smallSpacing
            rowSpacing: Kirigami.Units.smallSpacing

            Kirigami.Icon {
                source: "list-add"
                color: tile.hovered ? Kirigami.Theme.highlightColor : Kirigami.Theme.textColor
                Layout.alignment: Qt.AlignCenter
                Layout.preferredWidth: Kirigami.Units.iconSizes.medium
                Layout.preferredHeight: Kirigami.Units.iconSizes.medium
            }
            QQC2.Label {
                text: tile.text
                Layout.alignment: Qt.AlignCenter
            }
        }
    }

    background: Rectangle {
        radius: Kirigami.Units.cornerRadius
        color: tile.pressed ? Qt.alpha(Kirigami.Theme.highlightColor, 0.2)
             : tile.hovered ? Qt.alpha(Kirigami.Theme.highlightColor, 0.1) : "transparent"
        border.width: 1
        border.color: tile.hovered || tile.visualFocus
            ? Kirigami.Theme.highlightColor
            : Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.2)
    }
}
