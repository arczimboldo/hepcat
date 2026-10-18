// A titled group of strips, e.g. "Applications" or "Buses". Lays its strips
// out in one row, in columns of tiles that fill the height, or in rows that
// wrap at the section's width.

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: section

    property string title
    property string placeholder
    property bool empty: false
    // Strips wrap into rows (the page scrolls down) instead of forming one row.
    property bool wrap: false
    property bool compact: false
    // Width every strip gets (the widest one's), and in wrapping fader rows
    // their height.
    readonly property real stripWidth: naturalWidth
    readonly property real stripHeight: Kirigami.Units.gridUnit * 16
    property bool canMoveBack: false
    property bool canMoveForward: false
    // Strips of this section that are hidden right now.
    property int hiddenCount: 0
    default property alias strips: grid.data

    signal hideRequested()
    signal moveRequested(int delta)
    signal showHiddenRequested()

    spacing: Kirigami.Units.smallSpacing

    // Tallest tile, to work out how many fit in a column, and the widest
    // strip, so every strip gets the same width.
    readonly property real tileHeight: largest("implicitHeight")
    readonly property real naturalWidth: largest("implicitWidth")
    function largest(prop) {
        let m = 0
        for (const child of grid.children)
            if (child.visible && child[prop] > m)
                m = child[prop]
        return m
    }

    RowLayout {
        id: header
        spacing: Kirigami.Units.smallSpacing
        Layout.fillWidth: section.wrap

        TapHandler {
            acceptedButtons: Qt.RightButton
            onTapped: sectionMenu.popup()
        }

        Kirigami.Heading {
            text: section.title
            level: 4
        }
        QQC2.ToolButton {
            icon.name: "overflow-menu"
            display: QQC2.AbstractButton.IconOnly
            text: "Section options"
            padding: 0
            Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium + Kirigami.Units.smallSpacing * 2
            Layout.preferredHeight: Layout.preferredWidth
            QQC2.ToolTip.text: "Hide or move “" + section.title + "” (also on right-click)"
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            onClicked: sectionMenu.visible ? sectionMenu.close() : sectionMenu.popup(this, 0, height)
        }
        // Makes clear that not everything is shown, and brings it back.
        QQC2.ToolButton {
            visible: section.hiddenCount > 0
            text: section.hiddenCount + " hidden"
            icon.name: "view-hidden"
            font: Kirigami.Theme.smallFont
            QQC2.ToolTip.text: "Show strips you hid from “" + section.title + "”"
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            onClicked: section.showHiddenRequested()
        }
        Item { Layout.fillWidth: true }

        QQC2.Menu {
            id: sectionMenu
            QQC2.MenuItem {
                text: "Hide “" + section.title + "”"
                icon.name: "view-hidden"
                onTriggered: section.hideRequested()
            }
            QQC2.MenuItem {
                text: section.wrap ? "Move Up" : "Move Left"
                icon.name: section.wrap ? "go-up" : "go-previous"
                enabled: section.canMoveBack
                onTriggered: section.moveRequested(-1)
            }
            QQC2.MenuItem {
                text: section.wrap ? "Move Down" : "Move Right"
                icon.name: section.wrap ? "go-down" : "go-next"
                enabled: section.canMoveForward
                onTriggered: section.moveRequested(1)
            }
        }
    }

    GridLayout {
        id: grid
        readonly property real cell: section.stripWidth + columnSpacing
        // One row of fader strips; columns of tiles that fill the height; or
        // as many columns as fit the width.
        flow: !section.wrap && section.compact ? GridLayout.TopToBottom : GridLayout.LeftToRight
        rows: !section.wrap && section.compact
            ? Math.max(1, Math.floor((section.height - header.height - section.spacing * 2 + rowSpacing)
                                     / (section.tileHeight + rowSpacing)))
            : -1
        columns: section.wrap ? Math.max(1, Math.floor((section.width + columnSpacing) / cell)) : -1
        columnSpacing: Kirigami.Units.smallSpacing
        rowSpacing: Kirigami.Units.smallSpacing
        Layout.fillHeight: !section.wrap
        Layout.alignment: Qt.AlignTop | Qt.AlignLeft

        QQC2.Label {
            visible: section.empty
            text: section.placeholder
            opacity: 0.6
            wrapMode: Text.WordWrap
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            Layout.preferredWidth: Kirigami.Units.gridUnit * 8
            Layout.fillHeight: !section.wrap && !section.compact
            Layout.preferredHeight: section.wrap || section.compact ? Kirigami.Units.gridUnit * 3 : -1
        }
    }

    // Lets the section take the full height, so tiles can fill it.
    Item {
        visible: !section.wrap
        Layout.fillHeight: true
    }
}
