// Эмулятор терминала (VT100/xterm-подмножество): разбор потока байт в сетку ячеек.
// Не зависит от QML и сокетов — тестируется отдельно (tests/tst_terminal.cpp).
#pragma once

#include <QByteArray>
#include <QObject>
#include <QString>
#include <QVector>
#include <deque>

struct TermColor {
    // kind: 0 — по умолчанию, 1 — палитра 0..255, 2 — RGB
    quint8 kind = 0;
    quint8 r = 0, g = 0, b = 0; // для kind 1 индекс в r
    bool operator==(const TermColor &o) const { return kind == o.kind && r == o.r && g == o.g && b == o.b; }
    bool operator!=(const TermColor &o) const { return !(*this == o); }
};

struct TermCell {
    enum Attr : quint16 { Bold = 1, Dim = 2, Italic = 4, Underline = 8, Blink = 16, Inverse = 32, Hidden = 64, Strike = 128 };
    char32_t ch = U' ';
    quint16 attrs = 0;
    quint8 width = 1; // 0 — правая половина широкого символа
    TermColor fg, bg;
    bool operator==(const TermCell &o) const
    {
        return ch == o.ch && attrs == o.attrs && width == o.width && fg == o.fg && bg == o.bg;
    }
};

using TermLine = QVector<TermCell>;

class TermEmulator : public QObject {
    Q_OBJECT
public:
    explicit TermEmulator(int cols = 80, int rows = 24, QObject *parent = nullptr);

    int cols() const { return m_cols; }
    int rows() const { return m_rows; }
    int scrollbackSize() const { return int(m_scrollback.size()); }
    // Строка вида: row от 0 (верх экрана) при смещении 0; отрицательные — история.
    const TermLine &line(int row) const;
    int cursorX() const { return m_x; }
    int cursorY() const { return m_y; }
    bool cursorVisible() const { return m_cursorVisible; }
    bool altScreen() const { return m_alt; }
    bool appCursorKeys() const { return m_appCursor; }
    bool bracketedPaste() const { return m_bracketedPaste; }
    bool reverseVideo() const { return m_reverseVideo; }
    // 0 — выкл., 1000 — нажатия, 1002 — перетаскивание, 1003 — любое движение
    int mouseMode() const { return m_mouseMode; }
    bool mouseSgr() const { return m_mouseSgr; }
    QString title() const { return m_title; }
    QString cwd() const { return m_cwd; }
    QString textOfRow(int row) const;
    QString text(int fromRow, int fromCol, int toRow, int toCol) const;

    void feed(const QByteArray &data);
    void resize(int cols, int rows);
    void reset();
    // Сообщение «вне потока» (например, «сессия отключена») — выводится как есть, с переводом строки.
    void notice(const QString &text, int ansiColor = 3);

signals:
    void changed();
    void titleChanged(const QString &title);
    void bell();
    void response(const QByteArray &bytes);   // ответ эмулятора (DSR, DA…) — отправить в сессию
    void modesChanged();

private:
    enum PState { Ground, Esc, EscInter, Csi, Osc, OscEsc, Str, StrEsc };

    void put(char32_t cp);
    void execute(quint8 c);
    void escDispatch(char c);
    void csiDispatch(char fin);
    void oscDispatch();
    void sgr();
    void setMode(bool priv, int mode, bool on);
    void linefeed();
    void reverseIndex();
    void scrollUp(int n);
    void scrollDown(int n);
    void eraseLine(int mode);
    void eraseDisplay(int mode);
    void clearRow(int y, int from, int to);
    void insertChars(int n);
    void deleteChars(int n);
    void insertLines(int n);
    void deleteLines(int n);
    void moveTo(int x, int y);
    void clampCursor();
    void switchAlt(bool on, bool saveCursor);
    TermCell blank() const;
    int param(int i, int def) const;
    static int charWidth(char32_t cp);

    int m_cols, m_rows;
    QVector<TermLine> m_main, m_altBuf;
    QVector<TermLine> *m_screen;
    std::deque<TermLine> m_scrollback;
    int m_x = 0, m_y = 0;
    bool m_wrapPending = false;
    TermCell m_pen;
    int m_top = 0, m_bot = 0;
    bool m_cursorVisible = true, m_autowrap = true, m_origin = false, m_insert = false, m_newline = false;
    bool m_appCursor = false, m_bracketedPaste = false, m_alt = false, m_reverseVideo = false;
    int m_mouseMode = 0;
    bool m_mouseSgr = false;
    struct Saved { int x = 0, y = 0; TermCell pen; bool origin = false, wrap = false; int g = 0; } m_saved, m_savedAlt;
    QVector<bool> m_tabs;
    int m_charset[2] = {0, 0}; // 0 — ASCII, 1 — DEC графика
    int m_gl = 0;
    QString m_title, m_cwd;
    char32_t m_lastPrinted = 0;

    PState m_state = Ground;
    QByteArray m_inter;     // промежуточные байты/префикс CSI (?, >, пробел)
    QVector<int> m_params;
    bool m_haveParam = false;
    bool m_semiPending = false;
    QByteArray m_str;       // OSC/DCS содержимое
    int m_utf8Need = 0;
    char32_t m_utf8Cp = 0;
    int m_maxScrollback = 5000;
    TermLine m_emptyLine;
};
