// QML-элемент терминала: рисует сетку эмулятора, принимает клавиатуру/мышь/вставку.
#pragma once

#include "TerminalSession.h"

#include <QFont>
#include <QPointer>
#include <QQuickPaintedItem>
#include <QtQml/qqmlregistration.h>

class TerminalItem : public QQuickPaintedItem {
    Q_OBJECT
    QML_ELEMENT
    Q_PROPERTY(TerminalSession *session READ session WRITE setSession NOTIFY sessionChanged)
    Q_PROPERTY(int fontSize READ fontSize WRITE setFontSize NOTIFY fontSizeChanged)
    Q_PROPERTY(qreal backgroundOpacity READ backgroundOpacity WRITE setBackgroundOpacity NOTIFY backgroundOpacityChanged)
    Q_PROPERTY(int cols READ cols NOTIFY gridChanged)
    Q_PROPERTY(int rows READ rows NOTIFY gridChanged)
    Q_PROPERTY(bool hasSelection READ hasSelection NOTIFY selectionChanged)
public:
    explicit TerminalItem(QQuickItem *parent = nullptr);

    TerminalSession *session() const { return m_session; }
    void setSession(TerminalSession *s);
    int fontSize() const { return m_fontSize; }
    void setFontSize(int px);
    qreal backgroundOpacity() const { return m_bgOpacity; }
    void setBackgroundOpacity(qreal o);
    int cols() const { return m_cols; }
    int rows() const { return m_rows; }
    bool hasSelection() const { return m_selActive; }

    void paint(QPainter *p) override;

    Q_INVOKABLE void copy();
    Q_INVOKABLE void paste();
    Q_INVOKABLE void pasteText(const QString &text);
    Q_INVOKABLE void zoom(int delta);
    Q_INVOKABLE void scrollToBottom();
    Q_INVOKABLE QString screenText() const;

    // Для тестов: кодирование клавиши в байты терминала
    static QByteArray encodeKey(const QKeyEvent *e, bool appCursor);

signals:
    void sessionChanged();
    void fontSizeChanged();
    void backgroundOpacityChanged();
    void gridChanged();
    void selectionChanged();
    void multilinePaste(const QString &text);   // вставка с переводами строк — нужно подтверждение

protected:
    void keyPressEvent(QKeyEvent *e) override;
    void inputMethodEvent(QInputMethodEvent *e) override;
    QVariant inputMethodQuery(Qt::InputMethodQuery q) const override;
    void mousePressEvent(QMouseEvent *e) override;
    void mouseMoveEvent(QMouseEvent *e) override;
    void mouseReleaseEvent(QMouseEvent *e) override;
    void mouseDoubleClickEvent(QMouseEvent *e) override;
    void wheelEvent(QWheelEvent *e) override;
    void geometryChange(const QRectF &n, const QRectF &o) override;
    void focusInEvent(QFocusEvent *e) override;
    void focusOutEvent(QFocusEvent *e) override;
    bool event(QEvent *e) override;

private:
    void updateMetrics();
    void updateGrid();
    QColor resolve(const TermColor &c, bool fg, bool bold) const;
    QPoint cellAt(const QPointF &pos) const;
    void sendMouse(int button, const QPoint &cell, bool press, int mods, bool motion = false);
    void clearSelection();
    bool inSelection(int row, int col) const;
    void send(const QByteArray &b);

    QPointer<TerminalSession> m_session;
    QFont m_font, m_bold;
    int m_fontSize = 13;
    qreal m_bgOpacity = 0.62;
    qreal m_cw = 8, m_ch = 16, m_ascent = 12;
    int m_cols = 80, m_rows = 24;
    int m_scroll = 0;
    int m_lastScrollback = 0;
    bool m_selActive = false, m_selecting = false;
    QPoint m_selA, m_selB; // (col,row) в координатах эмулятора (row может быть < 0)
    int m_pressButtons = 0;
};
