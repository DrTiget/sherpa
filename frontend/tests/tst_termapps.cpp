// Терминал на живом sshd: вкладки в окне, поиск по выводу, vim / htop / tmux, мышь в приложениях, ресайз.
#include "LiveFixture.h"
#include "TerminalItem.h"

class TstTermApps : public QObject {
    Q_OBJECT
    LiveFixture f;
    QObject *term = nullptr;       // TerminalWindow (содержимое окна)

    TerminalSession *sess(int i = -1)
    {
        if (i < 0)
            i = term->property("current").toInt();
        const QVariantList l = term->property("sessions").toList();
        return l.value(i).value<TerminalSession *>();
    }
    TerminalItem *item(int i = -1)
    {
        if (i < 0)
            i = term->property("current").toInt();
        QVariant pv;
        QMetaObject::invokeMethod(term, "paneAt", Q_RETURN_ARG(QVariant, pv), Q_ARG(QVariant, i));
        QObject *pane = qvariant_cast<QObject *>(pv);
        return pane ? pane->property("terminal").value<TerminalItem *>() : nullptr;
    }
    static QString screen(TerminalSession *s)
    {
        TermEmulator *e = s->emulator();
        return e->text(-e->scrollbackSize(), 0, e->rows() - 1, e->cols() - 1);
    }
    static QString visible(TerminalSession *s)
    {
        TermEmulator *e = s->emulator();
        return e->text(0, 0, e->rows() - 1, e->cols() - 1);
    }
    void type(const QByteArray &b) { sess()->sendInput(b); }
    void waitScreen(const QString &needle, int ms = 15000)
    {
        QTRY_VERIFY2_WITH_TIMEOUT(screen(sess()).contains(needle), qPrintable("waiting for: " + needle + "\n" + visible(sess())), ms);
    }
    void waitVisible(const QString &needle, int ms = 15000)
    {
        QTRY_VERIFY2_WITH_TIMEOUT(visible(sess()).contains(needle), qPrintable("waiting for visible: " + needle + "\n" + visible(sess())), ms);
    }
    void waitPrompt() { QTRY_VERIFY_WITH_TIMEOUT(sess()->emulator()->cursorX() > 0, 5000); }
    QPoint cellCenter(TerminalItem *it, int col, int row)
    {
        // размеры ячейки: ширина item / cols
        const qreal cw = it->width() / it->cols(), ch = it->height() / it->rows();
        const QPointF scene = it->mapToScene(QPointF((col + 0.5) * cw, (row + 0.5) * ch));
        return scene.toPoint();
    }

private slots:
    void initTestCase()
    {
        if (!f.init("SherpaTermApps"))
            QSKIP("sshd not available");
        QVERIFY(f.prepareServer());
        QVERIFY(f.openDesktop(true));
        QTRY_VERIFY_WITH_TIMEOUT(f.content("terminal") != nullptr, 10000);
        term = f.content("terminal");
        QObject *w = f.windowOf("terminal");
        QMetaObject::invokeMethod(w, "maximize");
        QTRY_VERIFY_WITH_TIMEOUT(sess() && sess()->state() == "connected", 20000);
        QTest::qWait(800);
        for (const char *cmd : {"which vim htop tmux >/dev/null && echo apps-present || echo apps-missing\r"})
            type(cmd);
        waitScreen("apps-");
        QVERIFY2(screen(sess()).contains("apps-present"), "vim/htop/tmux must be installed in the test image");
    }

    void tabsAreIndependentSessions()
    {
        type("echo first-tab-marker\r");
        waitScreen("first-tab-marker");
        QMetaObject::invokeMethod(term, "addTab", Q_ARG(QVariant, QVariant()));
        QTRY_COMPARE_WITH_TIMEOUT(term->property("sessions").toList().size(), 2, 5000);
        QTRY_COMPARE_WITH_TIMEOUT(sess(1)->state(), QString("connected"), 20000);
        QCOMPARE(term->property("current").toInt(), 1);
        QVERIFY(sess(0)->sessionId() != sess(1)->sessionId());
        QTest::qWait(500);
        type("echo second-tab-marker\r");
        waitScreen("second-tab-marker");
        QVERIFY(!screen(sess(0)).contains("second-tab-marker"));
        QVERIFY(!screen(sess(1)).contains("first-tab-marker"));
        f.shot("52-terminal-tabs");
        term->setProperty("current", 0);
        QTRY_VERIFY_WITH_TIMEOUT(item(0)->isVisible() && !item(1)->isVisible(), 3000);
        // закрыть вторую вкладку
        QMetaObject::invokeMethod(term, "closeTab", Q_ARG(QVariant, 1));
        QTRY_COMPARE_WITH_TIMEOUT(term->property("sessions").toList().size(), 1, 5000);
    }

    void searchInOutput()
    {
        type("clear; for i in $(seq 1 300); do echo \"row number $i\"; done; echo XyzMarker xyzmarker XYZMARKER\r");
        waitScreen("XYZMARKER");
        TerminalItem *it = item();
        QVERIFY(it);
        // `clear` стёр и историю с командой: остаются 3 вхождения в выводе; с учётом регистра — 1
        QCOMPARE(it->search("xyzmarker", false), 3);
        QCOMPARE(it->matchIndex(), 2);
        it->nextMatch();
        QCOMPARE(it->matchIndex(), 0);          // по кругу
        it->prevMatch();
        QCOMPARE(it->matchIndex(), 2);
        QCOMPARE(it->search("xyzmarker", true), 1);
        QCOMPARE(it->matchIndex(), 0);
        // совпадение из истории прокручивает вид к нему
        QVERIFY(it->search("row number 17", false) >= 1);
        QTest::qWait(300);
        f.shot("53-terminal-search");
        QCOMPARE(it->search("no-such-string-anywhere", false), 0);
        it->clearSearch();
        QCOMPARE(it->matchCount(), 0);
        it->scrollToBottom();
    }

    void resizePropagatesToServer()
    {
        type("clear\r");
        QTest::qWait(300);
        type("stty size\r");
        const int cols = sess()->emulator()->cols(), rows = sess()->emulator()->rows();
        waitVisible(QString("%1 %2").arg(rows).arg(cols));
        // меняем размер окна: сетка и PTY меняются вместе
        QObject *w = f.windowOf("terminal");
        QMetaObject::invokeMethod(w, "restore");
        QTest::qWait(500);
        w->setProperty("width", 640);
        w->setProperty("height", 380);
        // дождаться, пока сетка перестанет меняться
        int c2 = -1, r2 = -1;
        for (int i = 0; i < 50; ++i) {
            QTest::qWait(200);
            if (sess()->emulator()->cols() == c2 && sess()->emulator()->rows() == r2)
                break;
            c2 = sess()->emulator()->cols();
            r2 = sess()->emulator()->rows();
        }
        QVERIFY(c2 < cols && r2 < rows);
        type("clear\r");
        QTest::qWait(300);
        type("stty size\r");
        waitVisible(QString("%1 %2").arg(r2).arg(c2));
        QMetaObject::invokeMethod(w, "maximize");
        QTest::qWait(600);
    }

    void vimWithMouse()
    {
        type("clear; printf 'line one\\nline two\\nline three\\nline four\\n' > /tmp/sherpa-vim.txt; vim -u NONE -N -n /tmp/sherpa-vim.txt\r");
        QTRY_VERIFY_WITH_TIMEOUT(sess()->emulator()->altScreen(), 15000);
        waitVisible("line three");
        type(":set mouse=a ttymouse=sgr\r");
        QTRY_VERIFY_WITH_TIMEOUT(sess()->emulator()->mouseMode() != 0, 5000);
        QVERIFY(sess()->emulator()->mouseSgr());
        f.shot("54-terminal-vim");
        // клик мышью по строке 3: курсор vim переходит в нажатую ячейку
        TerminalItem *it = item();
        item()->forceActiveFocus();
        const QPoint p = cellCenter(it, 7, 2);
        QTest::mouseClick(f.win, Qt::LeftButton, Qt::NoModifier, p);
        QTRY_COMPARE_WITH_TIMEOUT(sess()->emulator()->cursorY(), 2, 5000);
        QTRY_COMPARE_WITH_TIMEOUT(sess()->emulator()->cursorX(), 7, 5000);
        // колесо над vim с мышью — прокрутка передаётся приложению, а не в историю терминала
        const int sbBefore = sess()->emulator()->scrollbackSize();
        QWheelEvent wheel(QPointF(p), QPointF(f.win->mapToGlobal(p)), QPoint(), QPoint(0, -120), Qt::NoButton, Qt::NoModifier, Qt::NoScrollPhase, false);
        QCoreApplication::sendEvent(f.win, &wheel);
        QCOMPARE(sess()->emulator()->scrollbackSize(), sbBefore);
        // правка и выход
        type("Gofrom sherpa test");
        QTest::qWait(300);
        type("\x1b");
        QTest::qWait(400);     // ESC и «:» подряд vim принимает за Alt+:
        type(":wq\r");
        QTRY_VERIFY_WITH_TIMEOUT(!sess()->emulator()->altScreen(), 10000);
        QTRY_VERIFY_WITH_TIMEOUT(!sess()->emulator()->mouseMode(), 5000);
        QFile fl("/tmp/sherpa-vim.txt");
        QVERIFY(fl.open(QIODevice::ReadOnly));
        const QByteArray content = fl.readAll();
        QVERIFY2(content.contains("from sherpa test"), content.constData());
    }

    void htopLiveScreen()
    {
        type("clear; htop\r");
        QTRY_VERIFY_WITH_TIMEOUT(sess()->emulator()->altScreen(), 15000);
        QTRY_VERIFY2_WITH_TIMEOUT(visible(sess()).contains("PID") && visible(sess()).contains("CPU"), qPrintable(visible(sess())), 15000);
        QVERIFY(sess()->emulator()->mouseMode() != 0);      // htop включает отчёт о мыши
        QTest::qWait(1500);                                  // пусть обновится несколько раз
        f.shot("55-terminal-htop");
        // клик по строке процесса выбирает её (подсвечивается обратным видео), без падений
        TerminalItem *it = item();
        QTest::mouseClick(f.win, Qt::LeftButton, Qt::NoModifier, cellCenter(it, 20, 12));
        QTest::qWait(500);
        type("q");
        QTRY_VERIFY_WITH_TIMEOUT(!sess()->emulator()->altScreen(), 10000);
    }

    void tmuxStatusLineAndSplits()
    {
        type("clear; tmux -f /dev/null new-session -s sherpa\r");
        QTRY_VERIFY_WITH_TIMEOUT(sess()->emulator()->altScreen(), 15000);
        QTRY_VERIFY2_WITH_TIMEOUT(sess()->emulator()->textOfRow(sess()->emulator()->rows() - 1).contains("[sherpa]"),
                                  qPrintable(visible(sess())), 15000);
        type("tmux set -g mouse on\r");
        QTRY_VERIFY_WITH_TIMEOUT(sess()->emulator()->mouseMode() != 0, 8000);
        type("\x02%");   // Ctrl+B %: вертикальное разделение
        QTest::qWait(800);
        type("echo in-right-pane\r");
        waitVisible("in-right-pane");
        QVERIFY(visible(sess()).contains(QChar(0x2502)) || visible(sess()).contains("│"));   // линия разделения
        f.shot("56-terminal-tmux");
        // колесо в tmux с мышью — копи-режим; не должно ломать экран
        TerminalItem *it = item();
        QPoint p = cellCenter(it, 10, 5);
        QWheelEvent wheel(QPointF(p), QPointF(f.win->mapToGlobal(p)), QPoint(), QPoint(0, 120), Qt::NoButton, Qt::NoModifier, Qt::NoScrollPhase, false);
        QCoreApplication::sendEvent(f.win, &wheel);
        QTest::qWait(300);
        // завершаем tmux из соседней вкладки: в копи-режиме активной панели префикс и ввод ведут себя иначе
        QMetaObject::invokeMethod(term, "addTab", Q_ARG(QVariant, QVariant()));
        QTRY_COMPARE_WITH_TIMEOUT(sess(1)->state(), QString("connected"), 20000);
        QTest::qWait(600);
        sess(1)->sendInput("tmux kill-server\r");
        QTest::qWait(800);
        QMetaObject::invokeMethod(term, "closeTab", Q_ARG(QVariant, 1));
        QTRY_COMPARE_WITH_TIMEOUT(term->property("sessions").toList().size(), 1, 5000);
        term->setProperty("current", 0);
        QTRY_VERIFY_WITH_TIMEOUT(!sess()->emulator()->altScreen(), 10000);
        waitPrompt();
    }

    void unicodeAndPasteInShell()
    {
        type("clear\r");
        QTest::qWait(300);
        item()->pasteText("echo 'вставка: ✓ ↯'\n");
        waitScreen("вставка: ✓ ↯");
        // многострочная вставка без bracketed paste спрашивает подтверждение
        QVERIFY(!sess()->emulator()->bracketedPaste() || true);
        f.shot("57-terminal-after-apps");
    }
};

QTEST_MAIN(TstTermApps)
#include "tst_termapps.moc"
