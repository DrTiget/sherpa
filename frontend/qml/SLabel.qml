import QtQuick

// Текст в шрифтах и цветах темы. mono — моноширинный, sub — приглушённый, caps — подпись-заголовок.
Text {
    property bool mono: false
    property bool sub: false
    property bool caps: false
    color: sub || caps ? Theme.textSub : Theme.text
    font.family: mono ? Theme.fontMono : Theme.fontUi
    font.pixelSize: caps ? 10 : (mono ? Theme.sizeMono : Theme.sizeUi)
    font.bold: caps
    font.capitalization: caps ? Font.AllUppercase : Font.MixedCase
    font.letterSpacing: caps ? 0.8 : 0
    elide: Text.ElideRight
    textFormat: Text.PlainText
}
