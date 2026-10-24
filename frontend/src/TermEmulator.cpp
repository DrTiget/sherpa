#include "TermEmulator.h"

#include <QUrl>
#include <algorithm>

namespace {
const char32_t kDecGraphics[] = {
    // 0x5f..0x7e
    U' ', U'◆', U'▒', U'␉', U'␌', U'␍', U'␊', U'°', U'±', U'␤', U'␋', U'┘', U'┐', U'┌', U'└', U'┼',
    U'⎺', U'⎻', U'─', U'⎼', U'⎽', U'├', U'┤', U'┴', U'┬', U'│', U'≤', U'≥', U'π', U'≠', U'£', U'·'};
// 0x60..0x7f соответствует индексам 0..31 (0x5f → пробел особый случай)
}

TermEmulator::TermEmulator(int cols, int rows, QObject *parent) : QObject(parent), m_cols(cols), m_rows(rows)
{
    m_screen = &m_main;
    reset();
}

void TermEmulator::reset()
{
    m_main.assign(m_rows, TermLine(m_cols));
    m_altBuf.assign(m_rows, TermLine(m_cols));
    m_screen = &m_main;
    m_alt = false;
    m_scrollback.clear();
    m_pen = TermCell();
    m_x = m_y = 0;
    m_wrapPending = false;
    m_top = 0;
    m_bot = m_rows - 1;
    m_cursorVisible = m_autowrap = true;
    m_origin = m_insert = m_newline = m_appCursor = m_bracketedPaste = m_reverseVideo = false;
    m_mouseMode = 0;
    m_mouseSgr = false;
    m_charset[0] = m_charset[1] = 0;
    m_gl = 0;
    m_state = Ground;
    m_tabs.assign(m_cols, false);
    for (int i = 8; i < m_cols; i += 8)
        m_tabs[i] = true;
    m_saved = Saved();
    emit changed();
    emit modesChanged();
}

TermCell TermEmulator::blank() const
{
    TermCell c;
    c.bg = m_pen.bg; // стирание заливает текущим фоном
    return c;
}

const TermLine &TermEmulator::line(int row) const
{
    if (row >= 0 && row < m_rows)
        return (*m_screen)[row];
    const int sb = int(m_scrollback.size());
    const int idx = sb + row;
    if (row < 0 && idx >= 0)
        return m_scrollback[idx];
    return m_emptyLine;
}

QString TermEmulator::textOfRow(int row) const
{
    return text(row, 0, row, m_cols - 1);
}

QString TermEmulator::text(int r1, int c1, int r2, int c2) const
{
    QString out;
    for (int r = r1; r <= r2; ++r) {
        const TermLine &l = line(r);
        const int from = r == r1 ? c1 : 0;
        int to = r == r2 ? c2 : int(l.size()) - 1;
        to = qMin(to, int(l.size()) - 1);
        QString row;
        for (int c = from; c <= to; ++c) {
            if (l[c].width == 0)
                continue;
            row += QString::fromUcs4(&l[c].ch, 1);
        }
        if (r != r2) {
            while (row.endsWith(' '))
                row.chop(1);
            out += row + '\n';
        } else {
            out += row;
        }
    }
    return out;
}

void TermEmulator::resize(int cols, int rows)
{
    cols = qBound(1, cols, 1000);
    rows = qBound(1, rows, 1000);
    if (cols == m_cols && rows == m_rows)
        return;
    auto fit = [&](QVector<TermLine> &buf, bool toScrollback) {
        // уменьшение по высоте: верхние строки уходят в историю, пока курсор ниже новой границы
        while (buf.size() > rows) {
            if (toScrollback && m_y > rows - 1) {
                m_scrollback.push_back(buf.first());
                --m_y;
                buf.removeFirst();
            } else {
                buf.removeLast();
            }
        }
        while (buf.size() < rows)
            buf.append(TermLine(cols));
        for (TermLine &l : buf) {
            l.resize(cols);
            for (int i = 0; i < cols; ++i)
                if (l[i].width == 0 && i == 0)
                    l[i] = TermCell();
        }
    };
    const int oldRows = m_rows;
    Q_UNUSED(oldRows)
    fit(m_main, !m_alt);
    fit(m_altBuf, false);
    for (TermLine &l : m_scrollback)
        l.resize(cols);
    m_cols = cols;
    m_rows = rows;
    m_top = 0;
    m_bot = rows - 1;
    m_tabs.assign(cols, false);
    for (int i = 8; i < cols; i += 8)
        m_tabs[i] = true;
    clampCursor();
    m_wrapPending = false;
    emit changed();
}

void TermEmulator::clampCursor()
{
    m_x = qBound(0, m_x, m_cols - 1);
    m_y = qBound(0, m_y, m_rows - 1);
}

void TermEmulator::notice(const QString &text, int color)
{
    // переводим курсор в начало строки, печатаем цветной строкой, сбрасываем атрибуты
    const QByteArray b = QStringLiteral("\r\n\x1b[3%1m%2\x1b[0m\r\n").arg(color).arg(text).toUtf8();
    feed(b);
}

int TermEmulator::charWidth(char32_t cp)
{
    if (cp < 0x300)
        return 1;
    if ((cp >= 0x300 && cp <= 0x36F) || (cp >= 0x200B && cp <= 0x200F) || (cp >= 0xFE00 && cp <= 0xFE0F) || cp == 0x20E3)
        return 0;
    if ((cp >= 0x1100 && cp <= 0x115F) || (cp >= 0x2E80 && cp <= 0xA4CF) || (cp >= 0xAC00 && cp <= 0xD7A3)
        || (cp >= 0xF900 && cp <= 0xFAFF) || (cp >= 0xFE30 && cp <= 0xFE6F) || (cp >= 0xFF00 && cp <= 0xFF60)
        || (cp >= 0xFFE0 && cp <= 0xFFE6) || (cp >= 0x1F300 && cp <= 0x1F64F) || (cp >= 0x1F900 && cp <= 0x1F9FF)
        || (cp >= 0x20000 && cp <= 0x3FFFD))
        return 2;
    return 1;
}

// ---------------------------------------------------------------- разбор

void TermEmulator::feed(const QByteArray &data)
{
    for (const char ch : data) {
        const quint8 c = quint8(ch);
        // продолжение UTF-8
        if (m_utf8Need > 0) {
            if ((c & 0xC0) == 0x80) {
                m_utf8Cp = (m_utf8Cp << 6) | (c & 0x3F);
                if (--m_utf8Need == 0)
                    put(m_utf8Cp);
                continue;
            }
            m_utf8Need = 0;
            put(0xFFFD);
        }
        if (c == 0x18 || c == 0x1A) { // CAN / SUB
            m_state = Ground;
            continue;
        }
        if (c == 0x1B) {
            if (m_state == Osc || m_state == Str) {
                m_state = m_state == Osc ? OscEsc : StrEsc;
            } else {
                m_state = Esc;
                m_inter.clear();
            }
            continue;
        }
        switch (m_state) {
        case Ground:
            if (c < 0x20 || c == 0x7F) {
                execute(c);
            } else if (c < 0x80) {
                put(c);
            } else if ((c & 0xE0) == 0xC0) {
                m_utf8Need = 1; m_utf8Cp = c & 0x1F;
            } else if ((c & 0xF0) == 0xE0) {
                m_utf8Need = 2; m_utf8Cp = c & 0x0F;
            } else if ((c & 0xF8) == 0xF0) {
                m_utf8Need = 3; m_utf8Cp = c & 0x07;
            } else {
                put(0xFFFD);
            }
            break;
        case Esc:
            if (c == '[') {
                m_state = Csi; m_params.clear(); m_haveParam = false; m_semiPending = false; m_inter.clear();
            } else if (c == ']') {
                m_state = Osc; m_str.clear();
            } else if (c == 'P' || c == 'X' || c == '^' || c == '_') {
                m_state = Str; m_str.clear();
            } else if (c >= 0x20 && c <= 0x2F) {
                m_inter.append(char(c)); m_state = EscInter;
            } else {
                m_state = Ground;
                escDispatch(char(c));
            }
            break;
        case EscInter:
            if (c >= 0x20 && c <= 0x2F) {
                m_inter.append(char(c));
            } else {
                m_state = Ground;
                if (m_inter == "(" || m_inter == ")") {
                    const int g = m_inter == "(" ? 0 : 1;
                    m_charset[g] = c == '0' ? 1 : 0;
                }
            }
            break;
        case Csi:
            if (c >= '0' && c <= '9') {
                if (!m_haveParam) { m_params.append(0); m_haveParam = true; }
                m_semiPending = false;
                m_params.last() = qMin(m_params.last() * 10 + (c - '0'), 1000000);
            } else if (c == ';' || c == ':') {
                if (!m_haveParam) m_params.append(0);
                m_haveParam = false;
                m_semiPending = true;
            } else if (c >= 0x3C && c <= 0x3F) {
                m_inter.append(char(c));
            } else if (c >= 0x20 && c <= 0x2F) {
                m_inter.append(char(c));
            } else if (c >= 0x40 && c <= 0x7E) {
                m_state = Ground;
                csiDispatch(char(c));
            } else if (c < 0x20) {
                execute(c);
            }
            break;
        case Osc:
            if (c == 0x07) {
                m_state = Ground; oscDispatch();
            } else if (m_str.size() < 8192) {
                m_str.append(char(c));
            }
            break;
        case OscEsc:
            m_state = Ground;
            if (c == '\\') oscDispatch();
            else { m_state = Esc; }
            break;
        case Str:
            if (c == 0x07) m_state = Ground;
            break;
        case StrEsc:
            m_state = Ground;
            break;
        }
    }
    emit changed();
}

int TermEmulator::param(int i, int def) const
{
    if (i >= m_params.size() || m_params[i] == 0)
        return def;
    return m_params[i];
}

void TermEmulator::execute(quint8 c)
{
    switch (c) {
    case 0x07: emit bell(); break;
    case 0x08: if (m_x > 0) --m_x; m_wrapPending = false; break;
    case 0x09: {
        int x = m_x + 1;
        while (x < m_cols - 1 && !m_tabs[x]) ++x;
        m_x = qMin(x, m_cols - 1);
        m_wrapPending = false;
        break;
    }
    case 0x0A: case 0x0B: case 0x0C:
        linefeed();
        if (m_newline) m_x = 0;
        break;
    case 0x0D: m_x = 0; m_wrapPending = false; break;
    case 0x0E: m_gl = 1; break;
    case 0x0F: m_gl = 0; break;
    default: break;
    }
}

void TermEmulator::put(char32_t cp)
{
    if (m_charset[m_gl] == 1 && cp >= 0x5f && cp <= 0x7e)
        cp = cp == 0x5f ? U' ' : kDecGraphics[cp - 0x5f];
    const int w = charWidth(cp);
    if (w == 0) {
        // комбинирующий: добавляем к предыдущей ячейке нельзя (одна кодовая точка) — отбрасываем
        return;
    }
    if (m_wrapPending && m_autowrap) {
        m_x = 0;
        linefeed();
    }
    m_wrapPending = false;
    if (w == 2 && m_x == m_cols - 1) {
        if (m_autowrap) {
            (*m_screen)[m_y][m_x] = blank();
            m_x = 0;
            linefeed();
        } else {
            return;
        }
    }
    TermLine &l = (*m_screen)[m_y];
    if (m_insert) {
        for (int i = m_cols - 1; i >= m_x + w; --i)
            l[i] = l[i - w];
    }
    // перезапись половины широкого символа
    if (l[m_x].width == 0 && m_x > 0) l[m_x - 1] = blank();
    if (l[m_x].width == 2 && m_x + 1 < m_cols) l[m_x + 1] = blank();
    TermCell c = m_pen;
    c.ch = cp;
    c.width = quint8(w);
    l[m_x] = c;
    if (w == 2 && m_x + 1 < m_cols) {
        TermCell r = m_pen;
        r.ch = U' ';
        r.width = 0;
        l[m_x + 1] = r;
    }
    m_lastPrinted = cp;
    m_x += w;
    if (m_x >= m_cols) {
        m_x = m_cols - 1;
        m_wrapPending = true;
    }
}

void TermEmulator::linefeed()
{
    if (m_y == m_bot)
        scrollUp(1);
    else if (m_y < m_rows - 1)
        ++m_y;
}

void TermEmulator::reverseIndex()
{
    if (m_y == m_top)
        scrollDown(1);
    else if (m_y > 0)
        --m_y;
}

void TermEmulator::scrollUp(int n)
{
    n = qMin(n, m_bot - m_top + 1);
    for (int i = 0; i < n; ++i) {
        if (!m_alt && m_top == 0) {
            m_scrollback.push_back((*m_screen)[0]);
            if (int(m_scrollback.size()) > m_maxScrollback)
                m_scrollback.pop_front();
        }
        m_screen->remove(m_top);
        m_screen->insert(m_bot, TermLine(m_cols, blank()));
    }
}

void TermEmulator::scrollDown(int n)
{
    n = qMin(n, m_bot - m_top + 1);
    for (int i = 0; i < n; ++i) {
        m_screen->remove(m_bot);
        m_screen->insert(m_top, TermLine(m_cols, blank()));
    }
}

void TermEmulator::clearRow(int y, int from, int to)
{
    TermLine &l = (*m_screen)[y];
    const TermCell b = blank();
    for (int x = qMax(0, from); x <= qMin(to, m_cols - 1); ++x)
        l[x] = b;
}

void TermEmulator::eraseLine(int mode)
{
    if (mode == 0) clearRow(m_y, m_x, m_cols - 1);
    else if (mode == 1) clearRow(m_y, 0, m_x);
    else clearRow(m_y, 0, m_cols - 1);
}

void TermEmulator::eraseDisplay(int mode)
{
    if (mode == 0) {
        clearRow(m_y, m_x, m_cols - 1);
        for (int y = m_y + 1; y < m_rows; ++y) clearRow(y, 0, m_cols - 1);
    } else if (mode == 1) {
        for (int y = 0; y < m_y; ++y) clearRow(y, 0, m_cols - 1);
        clearRow(m_y, 0, m_x);
    } else if (mode == 2) {
        for (int y = 0; y < m_rows; ++y) clearRow(y, 0, m_cols - 1);
    } else if (mode == 3) {
        m_scrollback.clear();
    }
}

void TermEmulator::insertChars(int n)
{
    TermLine &l = (*m_screen)[m_y];
    n = qMin(n, m_cols - m_x);
    for (int x = m_cols - 1; x >= m_x + n; --x) l[x] = l[x - n];
    for (int x = m_x; x < m_x + n; ++x) l[x] = blank();
}

void TermEmulator::deleteChars(int n)
{
    TermLine &l = (*m_screen)[m_y];
    n = qMin(n, m_cols - m_x);
    for (int x = m_x; x < m_cols - n; ++x) l[x] = l[x + n];
    for (int x = m_cols - n; x < m_cols; ++x) l[x] = blank();
}

void TermEmulator::insertLines(int n)
{
    if (m_y < m_top || m_y > m_bot) return;
    n = qMin(n, m_bot - m_y + 1);
    for (int i = 0; i < n; ++i) {
        m_screen->remove(m_bot);
        m_screen->insert(m_y, TermLine(m_cols, blank()));
    }
    m_x = 0;
}

void TermEmulator::deleteLines(int n)
{
    if (m_y < m_top || m_y > m_bot) return;
    n = qMin(n, m_bot - m_y + 1);
    for (int i = 0; i < n; ++i) {
        m_screen->remove(m_y);
        m_screen->insert(m_bot, TermLine(m_cols, blank()));
    }
    m_x = 0;
}

void TermEmulator::moveTo(int x, int y)
{
    m_wrapPending = false;
    m_x = qBound(0, x, m_cols - 1);
    if (m_origin)
        m_y = qBound(m_top, y + m_top, m_bot);
    else
        m_y = qBound(0, y, m_rows - 1);
}

void TermEmulator::switchAlt(bool on, bool saveCursor)
{
    if (on == m_alt) return;
    if (on) {
        if (saveCursor) { m_saved.x = m_x; m_saved.y = m_y; m_saved.pen = m_pen; }
        m_altBuf.assign(m_rows, TermLine(m_cols));
        m_screen = &m_altBuf;
        m_alt = true;
    } else {
        m_screen = &m_main;
        m_alt = false;
        if (saveCursor) { m_x = m_saved.x; m_y = m_saved.y; m_pen = m_saved.pen; clampCursor(); }
    }
    m_top = 0; m_bot = m_rows - 1;
    m_wrapPending = false;
    emit modesChanged();
}

void TermEmulator::escDispatch(char c)
{
    switch (c) {
    case 'D': linefeed(); break;
    case 'E': m_x = 0; linefeed(); break;
    case 'M': reverseIndex(); break;
    case 'H': if (m_x < m_tabs.size()) m_tabs[m_x] = true; break;
    case '7': m_saved.x = m_x; m_saved.y = m_y; m_saved.pen = m_pen; m_saved.origin = m_origin; m_saved.wrap = m_wrapPending; break;
    case '8': m_x = m_saved.x; m_y = m_saved.y; m_pen = m_saved.pen; m_origin = m_saved.origin; clampCursor(); m_wrapPending = false; break;
    case 'c': reset(); break;
    case '=': case '>': break;
    default: break;
    }
}

void TermEmulator::setMode(bool priv, int mode, bool on)
{
    if (!priv) {
        if (mode == 4) m_insert = on;
        else if (mode == 20) m_newline = on;
        return;
    }
    switch (mode) {
    case 1: m_appCursor = on; emit modesChanged(); break;
    case 5: m_reverseVideo = on; break;
    case 6: m_origin = on; moveTo(0, 0); break;
    case 7: m_autowrap = on; break;
    case 25: m_cursorVisible = on; break;
    case 47: case 1047: switchAlt(on, false); break;
    case 1049: switchAlt(on, true); break;
    case 1000: case 1002: case 1003:
        m_mouseMode = on ? mode : 0; emit modesChanged(); break;
    case 1006: m_mouseSgr = on; emit modesChanged(); break;
    case 2004: m_bracketedPaste = on; emit modesChanged(); break;
    default: break;
    }
}

void TermEmulator::csiDispatch(char fin)
{
    if (m_semiPending) { m_params.append(0); m_semiPending = false; }
    const bool priv = m_inter.startsWith('?');
    const bool gt = m_inter.startsWith('>');
    const int p0 = param(0, 1);
    switch (fin) {
    case 'A': m_wrapPending = false; m_y = qMax(m_y - p0, m_origin ? m_top : 0); break;
    case 'B': case 'e': m_wrapPending = false; m_y = qMin(m_y + p0, m_origin ? m_bot : m_rows - 1); break;
    case 'C': case 'a': m_wrapPending = false; m_x = qMin(m_x + p0, m_cols - 1); break;
    case 'D': m_wrapPending = false; m_x = qMax(m_x - p0, 0); break;
    case 'E': m_x = 0; m_y = qMin(m_y + p0, m_rows - 1); m_wrapPending = false; break;
    case 'F': m_x = 0; m_y = qMax(m_y - p0, 0); m_wrapPending = false; break;
    case 'G': case '`': m_wrapPending = false; m_x = qBound(0, p0 - 1, m_cols - 1); break;
    case 'H': case 'f': moveTo(param(1, 1) - 1, param(0, 1) - 1); break;
    case 'd': { const int x = m_x; moveTo(x, p0 - 1); break; }
    case 'J': eraseDisplay(m_params.isEmpty() ? 0 : m_params[0]); break;
    case 'K': eraseLine(m_params.isEmpty() ? 0 : m_params[0]); break;
    case 'L': insertLines(p0); break;
    case 'M': deleteLines(p0); break;
    case '@': insertChars(p0); break;
    case 'P': deleteChars(p0); break;
    case 'X': clearRow(m_y, m_x, m_x + p0 - 1); break;
    case 'S': scrollUp(p0); break;
    case 'T': scrollDown(p0); break;
    case 'b': for (int i = 0; i < p0 && m_lastPrinted; ++i) put(m_lastPrinted); break;
    case 'm': if (!priv && !gt) sgr(); break;
    case 'h': case 'l': {
        const bool on = fin == 'h';
        for (int p : (m_params.isEmpty() ? QVector<int>{0} : m_params)) setMode(priv, p, on);
        break;
    }
    case 'r': {
        const int t = param(0, 1), b = param(1, m_rows);
        if (t < b && b <= m_rows) { m_top = t - 1; m_bot = b - 1; moveTo(0, 0); }
        break;
    }
    case 's': m_saved.x = m_x; m_saved.y = m_y; break;
    case 'u': m_x = m_saved.x; m_y = m_saved.y; clampCursor(); break;
    case 'g': {
        const int p = m_params.isEmpty() ? 0 : m_params[0];
        if (p == 3) m_tabs.fill(false); else if (p == 0 && m_x < m_tabs.size()) m_tabs[m_x] = false;
        break;
    }
    case 'n': {
        const int p = m_params.isEmpty() ? 0 : m_params[0];
        if (p == 5) emit response("\x1b[0n");
        else if (p == 6) emit response(QByteArray("\x1b[") + QByteArray::number((m_origin ? m_y - m_top : m_y) + 1) + ';' + QByteArray::number(m_x + 1) + 'R');
        break;
    }
    case 'c':
        if (gt) emit response("\x1b[>0;10;1c");
        else if (!priv) emit response("\x1b[?62;c");
        break;
    default: break;
    }
}

void TermEmulator::sgr()
{
    if (m_params.isEmpty()) { m_pen.attrs = 0; m_pen.fg = TermColor(); m_pen.bg = TermColor(); return; }
    for (int i = 0; i < m_params.size(); ++i) {
        const int p = m_params[i];
        auto set = [&](quint16 a, bool on) { if (on) m_pen.attrs |= a; else m_pen.attrs &= ~a; };
        if (p == 0) { m_pen.attrs = 0; m_pen.fg = TermColor(); m_pen.bg = TermColor(); }
        else if (p == 1) set(TermCell::Bold, true);
        else if (p == 2) set(TermCell::Dim, true);
        else if (p == 3) set(TermCell::Italic, true);
        else if (p == 4) set(TermCell::Underline, true);
        else if (p == 5 || p == 6) set(TermCell::Blink, true);
        else if (p == 7) set(TermCell::Inverse, true);
        else if (p == 8) set(TermCell::Hidden, true);
        else if (p == 9) set(TermCell::Strike, true);
        else if (p == 22) { set(TermCell::Bold, false); set(TermCell::Dim, false); }
        else if (p == 23) set(TermCell::Italic, false);
        else if (p == 24) set(TermCell::Underline, false);
        else if (p == 25) set(TermCell::Blink, false);
        else if (p == 27) set(TermCell::Inverse, false);
        else if (p == 28) set(TermCell::Hidden, false);
        else if (p == 29) set(TermCell::Strike, false);
        else if (p >= 30 && p <= 37) m_pen.fg = {1, quint8(p - 30), 0, 0};
        else if (p == 39) m_pen.fg = TermColor();
        else if (p >= 40 && p <= 47) m_pen.bg = {1, quint8(p - 40), 0, 0};
        else if (p == 49) m_pen.bg = TermColor();
        else if (p >= 90 && p <= 97) m_pen.fg = {1, quint8(p - 90 + 8), 0, 0};
        else if (p >= 100 && p <= 107) m_pen.bg = {1, quint8(p - 100 + 8), 0, 0};
        else if (p == 38 || p == 48) {
            TermColor c;
            if (i + 1 < m_params.size() && m_params[i + 1] == 5 && i + 2 < m_params.size()) {
                c = {1, quint8(qBound(0, m_params[i + 2], 255)), 0, 0};
                i += 2;
            } else if (i + 1 < m_params.size() && m_params[i + 1] == 2 && i + 4 < m_params.size()) {
                int k = i + 2;
                c = {2, quint8(m_params[k] & 255), quint8(m_params[k + 1] & 255), quint8(m_params[k + 2] & 255)};
                i = k + 2;
            } else {
                continue;
            }
            (p == 38 ? m_pen.fg : m_pen.bg) = c;
        }
    }
}

void TermEmulator::oscDispatch()
{
    const int semi = m_str.indexOf(';');
    const int code = semi < 0 ? m_str.toInt() : m_str.left(semi).toInt();
    const QString arg = semi < 0 ? QString() : QString::fromUtf8(m_str.mid(semi + 1));
    if (code == 0 || code == 2) {
        m_title = arg;
        emit titleChanged(m_title);
    } else if (code == 7) {
        // file://host/path
        const QUrl u(arg);
        m_cwd = u.isValid() && !u.path().isEmpty() ? u.path() : arg;
    }
}
