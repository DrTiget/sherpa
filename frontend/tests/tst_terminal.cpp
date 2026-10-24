// Тесты эмулятора терминала: управляющие последовательности, UTF-8, прокрутка, альтернативный экран.
#include "TermEmulator.h"

#include "TerminalItem.h"

#include <QKeyEvent>
#include <QSignalSpy>
#include <QtTest>

class TstTerminal : public QObject {
    Q_OBJECT
private slots:
    void plainText()
    {
        TermEmulator t(20, 5);
        t.feed("hello\r\nworld");
        QCOMPARE(t.textOfRow(0).trimmed(), QString("hello"));
        QCOMPARE(t.textOfRow(1).trimmed(), QString("world"));
        QCOMPARE(t.cursorX(), 5);
        QCOMPARE(t.cursorY(), 1);
    }
    void utf8SplitAcrossChunks()
    {
        TermEmulator t(20, 3);
        const QByteArray b = QString("Привет, мир ✓").toUtf8();
        t.feed(b.left(3));
        t.feed(b.mid(3));
        QCOMPARE(t.textOfRow(0).trimmed(), QString("Привет, мир ✓"));
    }
    void wideChars()
    {
        TermEmulator t(10, 2);
        t.feed(QString("日本a").toUtf8());
        QCOMPARE(t.line(0)[0].width, quint8(2));
        QCOMPARE(t.line(0)[1].width, quint8(0));
        QCOMPARE(t.cursorX(), 5);
    }
    void sgrColors()
    {
        TermEmulator t(20, 2);
        t.feed("\x1b[1;31mR\x1b[38;5;200mP\x1b[38;2;1;2;3mT\x1b[0mN");
        QCOMPARE(t.line(0)[0].fg.kind, quint8(1));
        QCOMPARE(t.line(0)[0].fg.r, quint8(1));
        QVERIFY(t.line(0)[0].attrs & TermCell::Bold);
        QCOMPARE(t.line(0)[1].fg.r, quint8(200));
        QCOMPARE(t.line(0)[2].fg.kind, quint8(2));
        QCOMPARE(t.line(0)[2].fg.g, quint8(2));
        QCOMPARE(t.line(0)[3].fg.kind, quint8(0));
        QCOMPARE(t.line(0)[3].attrs, quint16(0));
    }
    void cursorMovesAndErase()
    {
        TermEmulator t(10, 3);
        t.feed("abcdef\x1b[3D\x1b[K");
        QCOMPARE(t.textOfRow(0).trimmed(), QString("abc"));
        t.feed("\x1b[2;4Hx");
        QCOMPARE(t.line(1)[3].ch, char32_t('x'));
        t.feed("\x1b[2J");
        QCOMPARE(t.textOfRow(1).trimmed(), QString(""));
    }
    void scrollbackAndAutowrap()
    {
        TermEmulator t(5, 3);
        t.feed("1\r\n2\r\n3\r\n4\r\n5");
        QCOMPARE(t.scrollbackSize(), 2);
        QCOMPARE(t.textOfRow(-1).trimmed(), QString("2"));
        QCOMPARE(t.textOfRow(0).trimmed(), QString("3"));
        t.feed("\r\n123456");
        QCOMPARE(t.textOfRow(1).trimmed(), QString("12345"));
        QCOMPARE(t.textOfRow(2).trimmed(), QString("6"));
    }
    void altScreenRestores()
    {
        TermEmulator t(10, 3);
        t.feed("main");
        t.feed("\x1b[?1049h\x1b[2Jalt");
        QVERIFY(t.altScreen());
        QCOMPARE(t.textOfRow(0).trimmed(), QString("alt"));
        t.feed("\x1b[?1049l");
        QVERIFY(!t.altScreen());
        QCOMPARE(t.textOfRow(0).trimmed(), QString("main"));
        QCOMPARE(t.cursorX(), 4);
    }
    void scrollRegionAndInsertLines()
    {
        TermEmulator t(5, 4);
        t.feed("a\r\nb\r\nc\r\nd");
        t.feed("\x1b[2;3r\x1b[2;1H\x1b[L");
        QCOMPARE(t.textOfRow(0).trimmed(), QString("a"));
        QCOMPARE(t.textOfRow(1).trimmed(), QString(""));
        QCOMPARE(t.textOfRow(2).trimmed(), QString("b"));
        QCOMPARE(t.textOfRow(3).trimmed(), QString("d"));
    }
    void deviceStatusReports()
    {
        TermEmulator t(10, 5);
        QSignalSpy spy(&t, &TermEmulator::response);
        t.feed("ab\x1b[6n");
        QCOMPARE(spy.takeFirst().at(0).toByteArray(), QByteArray("\x1b[1;3R"));
        t.feed("\x1b[c");
        QVERIFY(spy.takeFirst().at(0).toByteArray().startsWith("\x1b[?"));
    }
    void titleAndModes()
    {
        TermEmulator t(10, 5);
        QSignalSpy spy(&t, &TermEmulator::titleChanged);
        t.feed("\x1b]0;user@host: ~\x07\x1b[?2004h\x1b[?1h\x1b[?1000h\x1b[?1006h");
        QCOMPARE(spy.takeFirst().at(0).toString(), QString("user@host: ~"));
        QVERIFY(t.bracketedPaste());
        QVERIFY(t.appCursorKeys());
        QCOMPARE(t.mouseMode(), 1000);
        QVERIFY(t.mouseSgr());
    }
    void decLineDrawing()
    {
        TermEmulator t(10, 2);
        t.feed("\x1b(0lqk\x1b(B");
        QCOMPARE(t.line(0)[0].ch, char32_t(U'┌'));
        QCOMPARE(t.line(0)[1].ch, char32_t(U'─'));
        QCOMPARE(t.line(0)[2].ch, char32_t(U'┐'));
    }
    void resizeKeepsContentAndCursor()
    {
        TermEmulator t(10, 5);
        t.feed("l1\r\nl2\r\nl3");
        t.resize(20, 2);
        QCOMPARE(t.rows(), 2);
        QCOMPARE(t.textOfRow(1).trimmed(), QString("l3"));
        QVERIFY(t.scrollbackSize() >= 1);   // верхние строки ушли в историю, а не пропали
        QVERIFY(t.cursorY() <= 1);
    }
    void keyEncoding()
    {
        auto enc = [](int key, Qt::KeyboardModifiers m, const QString &text = {}, bool app = false) {
            QKeyEvent e(QEvent::KeyPress, key, m, text);
            return TerminalItem::encodeKey(&e, app);
        };
        QCOMPARE(enc(Qt::Key_C, Qt::ControlModifier, "\x03"), QByteArray("\x03"));
        QCOMPARE(enc(Qt::Key_Up, Qt::NoModifier), QByteArray("\x1b[A"));
        QCOMPARE(enc(Qt::Key_Up, Qt::NoModifier, {}, true), QByteArray("\x1bOA"));
        QCOMPARE(enc(Qt::Key_Left, Qt::ControlModifier), QByteArray("\x1b[1;5D"));
        QCOMPARE(enc(Qt::Key_Return, Qt::NoModifier, "\r"), QByteArray("\r"));
        QCOMPARE(enc(Qt::Key_Backspace, Qt::NoModifier), QByteArray("\x7f"));
        QCOMPARE(enc(Qt::Key_F5, Qt::NoModifier), QByteArray("\x1b[15~"));
        QCOMPARE(enc(Qt::Key_X, Qt::AltModifier, "x"), QByteArray("\x1b" "x"));
        QCOMPARE(enc(Qt::Key_A, Qt::NoModifier, QString::fromUtf8("ф")), QString::fromUtf8("ф").toUtf8());
        QCOMPARE(enc(Qt::Key_Delete, Qt::NoModifier), QByteArray("\x1b[3~"));
    }
    void garbageDoesNotCrash()
    {
        TermEmulator t(10, 3);
        QByteArray junk;
        for (int i = 0; i < 20000; ++i) junk.append(char((i * 131 + 7) % 256));
        t.feed(junk);
        t.feed("\x1b[99999999;99999999H\x1b[-1m\x1b]\x1bP\x1b\\");
        QVERIFY(t.cursorX() < 10 && t.cursorY() < 3);
    }
};

QTEST_MAIN(TstTerminal)
#include "tst_terminal.moc"
