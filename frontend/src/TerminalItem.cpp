#include "TerminalItem.h"

#include <QClipboard>
#include <QFontMetricsF>
#include <QGuiApplication>
#include <QInputMethodEvent>
#include <QKeyEvent>
#include <QPainter>

namespace {
const QRgb kAnsi[16] = {0x1b1f2b, 0xff5370, 0x3dff8a, 0xffc53d, 0x5b9bff, 0xb388ff, 0x00e5ff, 0xd0d6e4,
                        0x5c6479, 0xff7a90, 0x7dffb0, 0xffd873, 0x8bb8ff, 0xcdb0ff, 0x6ff3ff, 0xffffff};
const QColor kDefaultFg(0xe6, 0xea, 0xf2);
const QColor kDefaultBg(0x0a, 0x0d, 0x14);
}

TerminalItem::TerminalItem(QQuickItem *parent) : QQuickPaintedItem(parent)
{
    setFlag(ItemHasContents, true);
    setFlag(ItemAcceptsInputMethod, true);
    setAcceptedMouseButtons(Qt::AllButtons);
    setActiveFocusOnTab(true);
    setAcceptHoverEvents(true);
    setRenderTarget(QQuickPaintedItem::Image);
    setAntialiasing(false);
    m_font.setFamilies({"JetBrains Mono", "DejaVu Sans Mono", "Consolas", "monospace"});
    m_font.setStyleHint(QFont::Monospace);
    updateMetrics();
}

void TerminalItem::setSession(TerminalSession *s)
{
    if (m_session == s)
        return;
    if (m_session)
        disconnect(m_session->emulator(), nullptr, this, nullptr);
    m_session = s;
    m_scroll = 0;
    clearSelection();
    if (s) {
        TermEmulator *e = s->emulator();
        m_lastScrollback = e->scrollbackSize();
        connect(e, &TermEmulator::changed, this, [this]() {
            const int sb = m_session->emulator()->scrollbackSize();
            if (m_scroll > 0 && sb > m_lastScrollback)
                m_scroll = qMin(sb, m_scroll + sb - m_lastScrollback);
            m_lastScrollback = sb;
            m_scroll = qMin(m_scroll, sb);
            update();
        });
        updateGrid();
    }
    emit sessionChanged();
    update();
}

void TerminalItem::setFontSize(int px)
{
    px = qBound(8, px, 40);
    if (px == m_fontSize)
        return;
    m_fontSize = px;
    updateMetrics();
    updateGrid();
    emit fontSizeChanged();
    update();
}

void TerminalItem::setBackgroundOpacity(qreal o)
{
    o = qBound(0.0, o, 1.0);
    if (qFuzzyCompare(o, m_bgOpacity))
        return;
    m_bgOpacity = o;
    emit backgroundOpacityChanged();
    update();
}

void TerminalItem::zoom(int delta) { setFontSize(delta == 0 ? 13 : m_fontSize + delta); }

void TerminalItem::updateMetrics()
{
    m_font.setPixelSize(m_fontSize);
    m_bold = m_font;
    m_bold.setBold(true);
    const QFontMetricsF fm(m_font);
    m_cw = qMax(1.0, fm.horizontalAdvance(QLatin1Char('M')));
    m_ch = qMax(1.0, qCeil(fm.height()) + 1.0);
    m_ascent = fm.ascent() + 0.5;
}

void TerminalItem::updateGrid()
{
    const int c = qMax(2, int(width() / m_cw));
    const int r = qMax(1, int(height() / m_ch));
    if (c == m_cols && r == m_rows)
        return;
    m_cols = c;
    m_rows = r;
    emit gridChanged();
    if (m_session)
        m_session->resize(c, r);
}

void TerminalItem::geometryChange(const QRectF &n, const QRectF &o)
{
    QQuickPaintedItem::geometryChange(n, o);
    if (n.size() != o.size() && width() > 0 && height() > 0)
        updateGrid();
}

QColor TerminalItem::resolve(const TermColor &c, bool fg, bool bold) const
{
    if (c.kind == 0)
        return fg ? kDefaultFg : kDefaultBg;
    if (c.kind == 2)
        return QColor(c.r, c.g, c.b);
    int i = c.r;
    if (fg && bold && i < 8)
        i += 8;
    if (i < 16)
        return QColor(kAnsi[i]);
    if (i < 232) {
        const int k = i - 16;
        auto lvl = [](int v) { return v == 0 ? 0 : 55 + v * 40; };
        return QColor(lvl(k / 36), lvl((k / 6) % 6), lvl(k % 6));
    }
    const int g = 8 + (i - 232) * 10;
    return QColor(g, g, g);
}

bool TerminalItem::inSelection(int row, int col) const
{
    if (!m_selActive)
        return false;
    QPoint a = m_selA, b = m_selB;
    if (a.y() > b.y() || (a.y() == b.y() && a.x() > b.x()))
        std::swap(a, b);
    if (row < a.y() || row > b.y())
        return false;
    if (row == a.y() && col < a.x())
        return false;
    if (row == b.y() && col > b.x())
        return false;
    return true;
}

void TerminalItem::paint(QPainter *p)
{
    const QRectF all(0, 0, width(), height());
    QColor bg = kDefaultBg;
    bg.setAlphaF(m_bgOpacity);
    p->setCompositionMode(QPainter::CompositionMode_Source);
    p->fillRect(all, bg);
    p->setCompositionMode(QPainter::CompositionMode_SourceOver);
    if (!m_session)
        return;
    const TermEmulator *e = m_session->emulator();
    const bool rev = e->reverseVideo();
    p->setFont(m_font);
    const int rows = qMin(m_rows, e->rows());
    for (int vr = 0; vr < rows; ++vr) {
        const int er = vr - m_scroll;
        const TermLine &l = e->line(er);
        const qreal y = vr * m_ch;
        const int n = qMin(int(l.size()), m_cols);
        // фон
        for (int x = 0; x < n; ++x) {
            const TermCell &c = l[x];
            const bool inv = (c.attrs & TermCell::Inverse) != 0;
            const bool sel = inSelection(er, x);
            QColor cb;
            bool draw = false;
            if (sel) {
                cb = QColor(54, 123, 240, 170);
                draw = true;
            } else if (inv != rev) {
                cb = resolve(c.fg, true, false);
                draw = true;
            } else if (c.bg.kind != 0) {
                cb = resolve(c.bg, false, false);
                draw = true;
            }
            if (draw)
                p->fillRect(QRectF(x * m_cw, y, m_cw * (c.width == 2 ? 2 : 1) + 0.5, m_ch), cb);
        }
        // текст по сериям одинакового стиля
        int x = 0;
        while (x < n) {
            const TermCell &c = l[x];
            if (c.width == 0 || c.ch == U' ' || (c.attrs & TermCell::Hidden)) {
                ++x;
                continue;
            }
            const bool inv = (c.attrs & TermCell::Inverse) != 0;
            const bool bold = (c.attrs & TermCell::Bold) != 0;
            const bool sel = inSelection(er, x);
            QColor fg;
            if (inv != rev)
                fg = c.bg.kind == 0 ? kDefaultBg : resolve(c.bg, false, false);
            else
                fg = resolve(c.fg, true, bold);
            if (sel)
                fg = QColor(255, 255, 255);
            if (c.attrs & TermCell::Dim)
                fg.setAlphaF(0.6);
            if (c.width == 2) {
                p->setFont(bold ? m_bold : m_font);
                p->setPen(fg);
                p->drawText(QPointF(x * m_cw, y + m_ascent), QString::fromUcs4(&c.ch, 1));
                x += 2;
                continue;
            }
            int end = x;
            QString run;
            while (end < n) {
                const TermCell &d = l[end];
                if (d.width != 1 || d.attrs != c.attrs || d.fg != c.fg || d.bg != c.bg || inSelection(er, end) != sel)
                    break;
                run += d.ch == U' ' ? QString(QLatin1Char(' ')) : QString::fromUcs4(&d.ch, 1);
                ++end;
            }
            p->setFont(bold ? m_bold : m_font);
            p->setPen(fg);
            if (run.size() == end - x) {
                p->drawText(QPointF(x * m_cw, y + m_ascent), run);
            } else {
                for (int i = x; i < end; ++i)
                    p->drawText(QPointF(i * m_cw, y + m_ascent), QString::fromUcs4(&l[i].ch, 1));
            }
            if (c.attrs & TermCell::Underline)
                p->drawLine(QPointF(x * m_cw, y + m_ch - 2), QPointF(end * m_cw, y + m_ch - 2));
            if (c.attrs & TermCell::Strike)
                p->drawLine(QPointF(x * m_cw, y + m_ch / 2), QPointF(end * m_cw, y + m_ch / 2));
            x = end;
        }
    }
    // курсор
    if (e->cursorVisible() && m_scroll == 0 && m_session->state() == "connected") {
        const QRectF r(e->cursorX() * m_cw, e->cursorY() * m_ch, m_cw, m_ch);
        if (hasActiveFocus()) {
            p->fillRect(r, QColor(0, 229, 255, 200));
            const TermCell &c = e->line(e->cursorY())[qMin(e->cursorX(), int(e->line(0).size()) - 1)];
            if (c.ch != U' ' && c.width) {
                p->setPen(kDefaultBg);
                p->setFont(m_font);
                p->drawText(QPointF(r.x(), r.y() + m_ascent), QString::fromUcs4(&c.ch, 1));
            }
        } else {
            p->setPen(QColor(0, 229, 255, 200));
            p->setBrush(Qt::NoBrush);
            p->drawRect(r.adjusted(0.5, 0.5, -0.5, -0.5));
        }
    }
}

// ---------------------------------------------------------------- ввод

void TerminalItem::send(const QByteArray &b)
{
    if (!m_session)
        return;
    if (m_scroll != 0) {
        m_scroll = 0;
        update();
    }
    m_session->sendInput(b);
}

QByteArray TerminalItem::encodeKey(const QKeyEvent *e, bool appCursor)
{
    const Qt::KeyboardModifiers m = e->modifiers();
    const bool shift = m & Qt::ShiftModifier, alt = m & Qt::AltModifier, ctrl = m & Qt::ControlModifier;
    const int mod = 1 + (shift ? 1 : 0) + (alt ? 2 : 0) + (ctrl ? 4 : 0);
    auto csi = [&](char fin, int num = 1) -> QByteArray {
        if (mod > 1)
            return "\x1b[" + QByteArray::number(num) + ';' + QByteArray::number(mod) + fin;
        return (num == 1 ? QByteArray("\x1b[") : QByteArray("\x1b[") + QByteArray::number(num)) + fin;
    };
    auto tilde = [&](int n) -> QByteArray {
        if (mod > 1)
            return "\x1b[" + QByteArray::number(n) + ';' + QByteArray::number(mod) + '~';
        return "\x1b[" + QByteArray::number(n) + '~';
    };
    auto arrow = [&](char c) -> QByteArray {
        if (mod > 1)
            return csi(c);
        return appCursor ? QByteArray("\x1bO") + c : QByteArray("\x1b[") + c;
    };
    switch (e->key()) {
    case Qt::Key_Up: return arrow('A');
    case Qt::Key_Down: return arrow('B');
    case Qt::Key_Right: return arrow('C');
    case Qt::Key_Left: return arrow('D');
    case Qt::Key_Home: return mod > 1 ? csi('H') : (appCursor ? QByteArray("\x1bOH") : QByteArray("\x1b[H"));
    case Qt::Key_End: return mod > 1 ? csi('F') : (appCursor ? QByteArray("\x1bOF") : QByteArray("\x1b[F"));
    case Qt::Key_Insert: return tilde(2);
    case Qt::Key_Delete: return tilde(3);
    case Qt::Key_PageUp: return tilde(5);
    case Qt::Key_PageDown: return tilde(6);
    case Qt::Key_F1: return mod > 1 ? csi('P') : QByteArray("\x1bOP");
    case Qt::Key_F2: return mod > 1 ? csi('Q') : QByteArray("\x1bOQ");
    case Qt::Key_F3: return mod > 1 ? csi('R') : QByteArray("\x1bOR");
    case Qt::Key_F4: return mod > 1 ? csi('S') : QByteArray("\x1bOS");
    case Qt::Key_F5: return tilde(15);
    case Qt::Key_F6: return tilde(17);
    case Qt::Key_F7: return tilde(18);
    case Qt::Key_F8: return tilde(19);
    case Qt::Key_F9: return tilde(20);
    case Qt::Key_F10: return tilde(21);
    case Qt::Key_F11: return tilde(23);
    case Qt::Key_F12: return tilde(24);
    case Qt::Key_Backtab: return "\x1b[Z";
    case Qt::Key_Tab: return alt ? QByteArray("\x1b\t") : QByteArray("\t");
    case Qt::Key_Return: case Qt::Key_Enter: return alt ? QByteArray("\x1b\r") : QByteArray("\r");
    case Qt::Key_Backspace: return ctrl ? QByteArray("\x08") : (alt ? QByteArray("\x1b\x7f") : QByteArray("\x7f"));
    case Qt::Key_Escape: return "\x1b";
    default: break;
    }
    if (ctrl) {
        const int k = e->key();
        if (k >= Qt::Key_A && k <= Qt::Key_Z)
            return (alt ? QByteArray("\x1b") : QByteArray()) + char(k - Qt::Key_A + 1);
        if (k == Qt::Key_Space || k == Qt::Key_At) return QByteArray(1, '\0');
        if (k == Qt::Key_BracketLeft) return "\x1b";
        if (k == Qt::Key_Backslash) return "\x1c";
        if (k == Qt::Key_BracketRight) return "\x1d";
        if (k == Qt::Key_AsciiCircum) return "\x1e";
        if (k == Qt::Key_Underscore || k == Qt::Key_Slash) return "\x1f";
    }
    const QString t = e->text();
    if (t.isEmpty() || (t.at(0).unicode() < 0x20 && t.at(0).unicode() != 0))
        return {};
    const QByteArray u = t.toUtf8();
    return alt ? QByteArray("\x1b") + u : u;
}

void TerminalItem::keyPressEvent(QKeyEvent *e)
{
    if (!m_session) {
        e->ignore();
        return;
    }
    const Qt::KeyboardModifiers m = e->modifiers();
    const bool ctrl = m & Qt::ControlModifier, shift = m & Qt::ShiftModifier;
    if (ctrl && shift && e->key() == Qt::Key_C) { copy(); return; }
    if (ctrl && shift && e->key() == Qt::Key_V) { paste(); return; }
    if (shift && !ctrl && e->key() == Qt::Key_Insert) { paste(); return; }
    if (ctrl && !shift && (e->key() == Qt::Key_Plus || e->key() == Qt::Key_Equal)) { zoom(1); return; }
    if (ctrl && !shift && e->key() == Qt::Key_Minus) { zoom(-1); return; }
    if (ctrl && !shift && e->key() == Qt::Key_0) { zoom(0); return; }
    if (shift && !ctrl && e->key() == Qt::Key_PageUp) {
        m_scroll = qMin(m_session->emulator()->scrollbackSize(), m_scroll + m_rows - 1);
        update();
        return;
    }
    if (shift && !ctrl && e->key() == Qt::Key_PageDown) {
        m_scroll = qMax(0, m_scroll - (m_rows - 1));
        update();
        return;
    }
    // сочетания рабочего стола (Ctrl+Alt+…, Ctrl+Tab, Ctrl+Shift+W) не перехватываем
    if (ctrl && (m & Qt::AltModifier)) { e->ignore(); return; }
    if (ctrl && e->key() == Qt::Key_Tab) { e->ignore(); return; }
    const QByteArray b = encodeKey(e, m_session->emulator()->appCursorKeys());
    if (b.isEmpty()) {
        e->ignore();
        return;
    }
    clearSelection();
    send(b);
    e->accept();
}

void TerminalItem::inputMethodEvent(QInputMethodEvent *e)
{
    if (!e->commitString().isEmpty())
        send(e->commitString().toUtf8());
    e->accept();
}

QVariant TerminalItem::inputMethodQuery(Qt::InputMethodQuery q) const
{
    if (q == Qt::ImCursorRectangle)
        return QRectF(m_session ? m_session->emulator()->cursorX() * m_cw : 0,
                      m_session ? m_session->emulator()->cursorY() * m_ch : 0, m_cw, m_ch);
    return QQuickItem::inputMethodQuery(q);
}

bool TerminalItem::event(QEvent *e)
{
    // Tab и Shift+Tab нужны терминалу, а не навигации фокуса
    if (e->type() == QEvent::ShortcutOverride && m_session) {
        auto *k = static_cast<QKeyEvent *>(e);
        const bool ctrl = k->modifiers() & Qt::ControlModifier;
        if (!(ctrl && (k->modifiers() & Qt::AltModifier)) && !(ctrl && k->key() == Qt::Key_Tab)
            && !(ctrl && (k->modifiers() & Qt::ShiftModifier) && k->key() == Qt::Key_W)) {
            e->accept();
            return true;
        }
    }
    return QQuickPaintedItem::event(e);
}

void TerminalItem::focusInEvent(QFocusEvent *e) { QQuickPaintedItem::focusInEvent(e); update(); }
void TerminalItem::focusOutEvent(QFocusEvent *e) { QQuickPaintedItem::focusOutEvent(e); update(); }

void TerminalItem::paste()
{
    const QString t = QGuiApplication::clipboard()->text();
    if (m_session && !m_session->emulator()->bracketedPaste() && t.trimmed().contains('\n')) {
        emit multilinePaste(t);
        return;
    }
    pasteText(t);
}

void TerminalItem::pasteText(const QString &textIn)
{
    if (!m_session || textIn.isEmpty())
        return;
    QString text = textIn;
    text.replace("\r\n", "\n");
    const bool bracketed = m_session->emulator()->bracketedPaste();
    if (!bracketed)
        text.replace('\n', '\r');
    QByteArray b = text.toUtf8();
    if (bracketed)
        b = "\x1b[200~" + b + "\x1b[201~";
    send(b);
}

void TerminalItem::copy()
{
    if (!m_session || !m_selActive)
        return;
    QPoint a = m_selA, b = m_selB;
    if (a.y() > b.y() || (a.y() == b.y() && a.x() > b.x()))
        std::swap(a, b);
    QGuiApplication::clipboard()->setText(m_session->emulator()->text(a.y(), a.x(), b.y(), b.x()));
}

QString TerminalItem::screenText() const
{
    if (!m_session)
        return {};
    return m_session->emulator()->text(0, 0, m_session->emulator()->rows() - 1, m_session->emulator()->cols() - 1);
}

void TerminalItem::scrollToBottom() { m_scroll = 0; update(); }

// ---------------------------------------------------------------- мышь

QPoint TerminalItem::cellAt(const QPointF &pos) const
{
    return QPoint(qBound(0, int(pos.x() / m_cw), m_cols - 1), qBound(0, int(pos.y() / m_ch), m_rows - 1));
}

void TerminalItem::clearSelection()
{
    if (m_selActive) {
        m_selActive = false;
        emit selectionChanged();
        update();
    }
}

void TerminalItem::sendMouse(int button, const QPoint &cell, bool press, int mods, bool motion)
{
    const TermEmulator *e = m_session->emulator();
    int cb = button;
    if (mods & Qt::ShiftModifier) cb |= 4;
    if (mods & Qt::AltModifier) cb |= 8;
    if (mods & Qt::ControlModifier) cb |= 16;
    if (motion) cb |= 32;
    QByteArray out;
    if (e->mouseSgr()) {
        out = "\x1b[<" + QByteArray::number(cb) + ';' + QByteArray::number(cell.x() + 1) + ';'
              + QByteArray::number(cell.y() + 1) + (press ? 'M' : 'm');
    } else {
        if (!press) cb = 3;
        if (cell.x() > 222 || cell.y() > 222) return;
        out = "\x1b[M" + QByteArray(1, char(32 + cb)) + QByteArray(1, char(33 + cell.x())) + QByteArray(1, char(33 + cell.y()));
    }
    m_session->sendInput(out);
}

void TerminalItem::mousePressEvent(QMouseEvent *e)
{
    forceActiveFocus();
    if (!m_session)
        return;
    const QPoint cell = cellAt(e->position());
    const TermEmulator *em = m_session->emulator();
    const bool reporting = em->mouseMode() != 0 && !(e->modifiers() & Qt::ShiftModifier);
    if (reporting) {
        const int b = e->button() == Qt::LeftButton ? 0 : e->button() == Qt::MiddleButton ? 1 : 2;
        m_pressButtons = b + 1;
        sendMouse(b, cell, true, e->modifiers());
        return;
    }
    if (e->button() == Qt::LeftButton) {
        clearSelection();
        m_selecting = true;
        m_selA = m_selB = QPoint(cell.x(), cell.y() - m_scroll);
        update();
    } else if (e->button() == Qt::MiddleButton) {
        paste();
    } else if (e->button() == Qt::RightButton) {
        if (m_selActive) { copy(); clearSelection(); } else paste();
    }
}

void TerminalItem::mouseMoveEvent(QMouseEvent *e)
{
    if (!m_session)
        return;
    const QPoint cell = cellAt(e->position());
    const TermEmulator *em = m_session->emulator();
    if (m_pressButtons && (em->mouseMode() == 1002 || em->mouseMode() == 1003)) {
        sendMouse(m_pressButtons - 1, cell, true, e->modifiers(), true);
        return;
    }
    if (m_selecting) {
        m_selB = QPoint(cell.x(), cell.y() - m_scroll);
        const bool was = m_selActive;
        m_selActive = m_selA != m_selB;
        if (was != m_selActive)
            emit selectionChanged();
        update();
    }
}

void TerminalItem::mouseReleaseEvent(QMouseEvent *e)
{
    if (!m_session)
        return;
    if (m_pressButtons) {
        sendMouse(m_pressButtons - 1, cellAt(e->position()), false, e->modifiers());
        m_pressButtons = 0;
        return;
    }
    if (m_selecting) {
        m_selecting = false;
        if (m_selActive)
            copy();   // как в X11: выделенное сразу в буфер
    }
}

void TerminalItem::mouseDoubleClickEvent(QMouseEvent *e)
{
    if (!m_session || m_pressButtons)
        return;
    const QPoint cell = cellAt(e->position());
    const int row = cell.y() - m_scroll;
    const TermLine &l = m_session->emulator()->line(row);
    auto isWord = [&](int x) { const char32_t c = l[x].ch; return c > 32 && c != U'"' && c != U'\'' && c != U'|' && c != U'(' && c != U')'; };
    if (cell.x() >= l.size() || !isWord(cell.x()))
        return;
    int a = cell.x(), b = cell.x();
    while (a > 0 && isWord(a - 1)) --a;
    while (b + 1 < l.size() && isWord(b + 1)) ++b;
    m_selA = QPoint(a, row);
    m_selB = QPoint(b, row);
    m_selActive = true;
    m_selecting = false;
    emit selectionChanged();
    copy();
    update();
}

void TerminalItem::wheelEvent(QWheelEvent *e)
{
    if (!m_session)
        return;
    const int steps = e->angleDelta().y() / 40;
    if (steps == 0)
        return;
    const TermEmulator *em = m_session->emulator();
    if (em->mouseMode() != 0 && !(e->modifiers() & Qt::ShiftModifier)) {
        for (int i = 0; i < qAbs(steps); ++i)
            sendMouse(steps > 0 ? 64 : 65, cellAt(e->position()), true, e->modifiers());
        return;
    }
    if (em->altScreen()) {
        const QByteArray k = em->appCursorKeys() ? (steps > 0 ? "\x1bOA" : "\x1bOB") : (steps > 0 ? "\x1b[A" : "\x1b[B");
        for (int i = 0; i < qAbs(steps); ++i)
            send(k);
        return;
    }
    m_scroll = qBound(0, m_scroll + steps, em->scrollbackSize());
    update();
}
