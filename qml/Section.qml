// A titled group of strips, e.g. "Applications" or "Buses".

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: section

    property string title
    property string subtitle
    property string placeholder
    property bool empty: false
    default property alias strips: row.data

    spacing: Kirigami.Units.smallSpacing

    Kirigami.Heading {
        text: section.title
        level: 4
    }
    QQC2.Label {
        text: section.subtitle
        visible: text.length > 0
        opacity: 0.7
        font: Kirigami.Theme.smallFont
        wrapMode: Text.WordWrap
        Layout.maximumWidth: Math.max(row.implicitWidth, Kirigami.Units.gridUnit * 12)
    }

    RowLayout {
        id: row
        spacing: Kirigami.Units.smallSpacing
        Layout.fillHeight: true

        QQC2.Label {
            visible: section.empty
            text: section.placeholder
            opacity: 0.6
            wrapMode: Text.WordWrap
            horizontalAlignment: Text.AlignHCenter
            Layout.preferredWidth: Kirigami.Units.gridUnit * 8
            Layout.fillHeight: true
            verticalAlignment: Text.AlignVCenter
        }
    }
}
