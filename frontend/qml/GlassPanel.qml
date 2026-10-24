import QtQuick
import QtQuick.Effects

// Полупрозрачная панель с размытием фона (MultiEffect поверх ShaderEffectSource).
// Если размытие недоступно или выключено — обычная заливка тех же цветов.
Item {
    id: root
    property color color: Theme.panelFill
    property real radius: 0
    property Item source: Theme.bgItem
    property color borderColor: "transparent"
    property int borderWidth: 0
    property bool blurred: Theme.blurOn && source !== null

    function refresh() {
        if (!blurred || !source)
            return
        var r = root.mapToItem(source, 0, 0, root.width, root.height)
        ses.sourceRect = Qt.rect(r.x, r.y, r.width, r.height)
        ses.scheduleUpdate()
    }
    onWidthChanged: refresh()
    onHeightChanged: refresh()
    onBlurredChanged: refresh()
    onVisibleChanged: if (visible) refresh()
    Component.onCompleted: refresh()
    Connections {
        target: root.source
        function onWidthChanged() { root.refresh() }
        function onHeightChanged() { root.refresh() }
    }

    ShaderEffectSource {
        id: ses
        visible: false
        sourceItem: root.blurred ? root.source : null
        live: false
        hideSource: false
    }
    Rectangle {
        id: mask
        anchors.fill: parent
        radius: root.radius
        visible: false
        layer.enabled: root.blurred && root.radius > 0
    }
    MultiEffect {
        anchors.fill: parent
        visible: root.blurred
        source: ses
        blurEnabled: true
        blurMax: 48
        blur: 1.0
        maskEnabled: root.radius > 0
        maskSource: mask
        autoPaddingEnabled: false
    }
    Rectangle {
        anchors.fill: parent
        radius: root.radius
        color: root.color
        border.color: root.borderColor
        border.width: root.borderWidth
    }
}
