// A volume slider with a peak meter beside it (vertical) or under it
// (horizontal).

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

GridLayout {
    id: fader

    property bool vertical: true
    // On the cubic scale, 1.0 is 100%.
    property real volume
    // Peak level 0..1 (-60..0 dB)
    property real level
    property bool mute
    // What the slider shows right now, also while it is being dragged.
    readonly property alias value: slider.value

    signal moved(real value)

    columns: vertical ? 2 : 1
    columnSpacing: Kirigami.Units.smallSpacing
    rowSpacing: 0

    QQC2.Slider {
        id: slider
        orientation: fader.vertical ? Qt.Vertical : Qt.Horizontal
        from: 0
        to: 1
        stepSize: 0.01
        Layout.fillHeight: fader.vertical
        Layout.fillWidth: !fader.vertical
        opacity: fader.mute ? 0.5 : 1
        onMoved: fader.moved(value)

        // Ignore echoes of our own changes while the handle is held.
        Binding on value {
            value: fader.volume
            when: !slider.pressed
            restoreMode: Binding.RestoreNone
        }
    }

    // Peak meter on a -60..0 dB scale: green, yellow from -12 dB, red from -3 dB.
    Rectangle {
        id: meter
        readonly property real thickness: Math.round(Kirigami.Units.smallSpacing * 1.5)
        Layout.fillHeight: fader.vertical
        Layout.fillWidth: !fader.vertical
        Layout.topMargin: fader.vertical ? slider.topPadding : 0
        Layout.bottomMargin: fader.vertical ? slider.bottomPadding : 0
        Layout.leftMargin: fader.vertical ? 0 : slider.leftPadding
        Layout.rightMargin: fader.vertical ? 0 : slider.rightPadding
        implicitWidth: thickness
        implicitHeight: thickness
        radius: thickness / 2
        color: Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.1)
        clip: true

        Rectangle {
            anchors.bottom: parent.bottom
            anchors.left: parent.left
            width: fader.vertical ? parent.width : parent.width * fader.level
            height: fader.vertical ? parent.height * fader.level : parent.height
            radius: parent.radius
            opacity: fader.mute ? 0.4 : 1
            color: fader.level > 0.95 ? Kirigami.Theme.negativeTextColor
                 : fader.level > 0.8 ? Kirigami.Theme.neutralTextColor
                 : Kirigami.Theme.positiveTextColor
        }
    }
}
