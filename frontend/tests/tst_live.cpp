// Сквозной тест на живом OpenSSH sshd (локально, случайный порт): проверка online / auth_failed,
// host key (новый, сменился), рабочий стол, терминал, монитор, раскладка, переподключение клиента демона.
// Снимает скриншоты в SHERPA_SHOTS_DIR. Без sshd — пропускается.
#include "Backend.h"
#include "CoreEnv.h"
#include "TestSshd.h"

#include <QFontDatabase>
#include <QJsonArray>
#include <QQmlApplicationEngine>
#include <QQuickStyle>
#include <QQuickItem>
#include <QQuickWindow>
#include <QtQml/qqmlextensionplugin.h>
#include <QtTest>

Q_IMPORT_QML_PLUGIN(SherpaPlugin)

class TstLive : public QObject {
    Q_OBJECT
    CoreEnv env;
    TestSshd sshd;
    QQmlApplicationEngine *engine = nullptr;
    QQuickWindow *win = nullptr;
    Backend *be = nullptr;
    QString dir, okId, badId;

    void shot(const QString &name)
    {
        QTest::qWait(500);
        const QImage img = win->grabWindow();
        QVERIFY(!img.isNull());
        QVERIFY(img.save(dir + "/" + name + ".png"));
    }
    QObject *prop(const char *n) { return win->property(n).value<QObject *>(); }
    QString stateOf(const QString &id) { return be->serverById(id).value("state").toString(); }
    DesktopState *desk() { return be->desk()->desktop(okId); }
    QObject *page()
    {
        QObject *rep = prop("desktopRepeater");
        QQuickItem *p = nullptr;
        QMetaObject::invokeMethod(rep, "itemAt", Q_RETURN_ARG(QQuickItem *, p), Q_ARG(int, 0));
        return p;
    }
    TerminalSession *firstTerminal()
    {
        for (const QVariant &w : page()->property("wins").toList()) {
            const QVariantMap m = w.toMap();
            if (m.value("kind") == "terminal")
                return m.value("content").value<QObject *>()->property("currentSession").value<TerminalSession *>();
        }
        return nullptr;
    }
    QString screen(TerminalSession *s)
    {
        TermEmulator *e = s->emulator();
        return e->text(-e->scrollbackSize(), 0, e->rows() - 1, e->cols() - 1);
    }
    int windowCount() { return page()->property("wins").toList().size(); }

private slots:
    void initTestCase()
    {
        if (TestSshd::find().isEmpty())
            QSKIP("sshd not installed");
        QVERIFY2(sshd.start(), "sshd did not start");
        // короткий путь для сокета демона
        qputenv("XDG_RUNTIME_DIR", env.dir.filePath("run").toUtf8());
        QDir().mkpath(env.dir.filePath("run"));
        QFile(env.dir.filePath("run")).setPermissions(QFile::ReadOwner | QFile::WriteOwner | QFile::ExeOwner);
        dir = qEnvironmentVariable("SHERPA_SHOTS_DIR", QDir::currentPath() + "/screenshots");
        QDir().mkpath(dir);
        QQuickStyle::setStyle("Basic");
        QCoreApplication::setOrganizationName("SherpaLive");
        QCoreApplication::setApplicationName("SherpaLive");
        QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, env.dir.filePath("settings"));
        for (const char *f : {"JetBrainsMono-Regular.ttf", "JetBrainsMono-Bold.ttf", "Inter-Regular.otf", "Inter-SemiBold.otf", "Inter-Bold.otf"})
            QFontDatabase::addApplicationFont(QStringLiteral(":/fonts/") + f);
        engine = new QQmlApplicationEngine;
        engine->loadFromModule("Sherpa", "Main");
        QVERIFY(!engine->rootObjects().isEmpty());
        win = qobject_cast<QQuickWindow *>(engine->rootObjects().first());
        win->resize(1360, 820);
        be = engine->singletonInstance<Backend *>("Sherpa", "Backend");
        QTRY_VERIFY_WITH_TIMEOUT(be->appState() == "firstrun", 10000);
        be->finishFirstRun();
        QTRY_VERIFY_WITH_TIMEOUT(be->appState() == "ready", 10000);

        CoreClient *c = be->client();
        auto mk = [&](const QString &name, const QString &keyPath) {
            QJsonObject in{{"name", name}, {"host", "127.0.0.1"}, {"port", sshd.port}, {"user", sshd.user},
                           {"tags", QJsonArray{"live"}},
                           {"auth", QJsonObject{{"method", "key_text"}, {"key_text", sshd.readKey(keyPath)}}}};
            auto r = c->runBlocking({"conn", "create"}, in, true);
            return r.data.value("connection").toMap().value("id").toString();
        };
        okId = mk("live-ok", sshd.clientKeyPath);
        badId = mk("live-denied", sshd.otherKeyPath);
        QVERIFY(!okId.isEmpty() && !badId.isEmpty());
        be->reload();
        QTRY_COMPARE_WITH_TIMEOUT(be->servers()->totalCount(), 2, 10000);
    }

    void unknownHostKeyThenAccept()
    {
        be->setSelectedId(okId);
        be->checkOne(okId);
        QTRY_VERIFY_WITH_TIMEOUT(!be->pendingHostKey().isEmpty(), 15000);
        QCOMPARE(be->pendingHostKey().value("kind").toString(), QString("unknown"));
        QVERIFY(be->pendingHostKey().value("fingerprint").toString().startsWith("SHA256:"));
        shot("21-live-hostkey-new");
        be->answerHostKey("accept");
        QTRY_COMPARE_WITH_TIMEOUT(stateOf(okId), QString("online"), 20000);
    }
    void checkAllOnlineAndAuthFailed()
    {
        be->checkAll();
        QTRY_VERIFY_WITH_TIMEOUT(!be->bulkChecking(), 30000);
        // у denied ещё нет доверенного ключа? ключ хоста общий (host:port) — уже принят, значит дойдёт до авторизации
        QCOMPARE(stateOf(okId), QString("online"));
        QCOMPARE(stateOf(badId), QString("auth_failed"));
        QVERIFY(be->serverById(badId).value("error_text").toString().contains("rejected"));
        be->setSelectedId(okId);
        shot("22-live-home-statuses");
    }
    void hostKeyChangedReplace()
    {
        sshd.stop();
        QVERIFY(sshd.newHostKey());
        QVERIFY(sshd.launch());
        be->checkOne(okId);
        QTRY_VERIFY_WITH_TIMEOUT(!be->pendingHostKey().isEmpty(), 15000);
        QCOMPARE(be->pendingHostKey().value("kind").toString(), QString("mismatch"));
        QVERIFY(!be->pendingHostKey().value("knownFingerprint").toString().isEmpty());
        QCOMPARE(stateOf(okId), QString("error"));
        shot("23-live-hostkey-changed");
        be->answerHostKey("accept");   // replace
        QTRY_COMPARE_WITH_TIMEOUT(stateOf(okId), QString("online"), 20000);
    }

    void desktopAndTerminal()
    {
        QMetaObject::invokeMethod(win, "openDesktop", Q_ARG(QVariant, okId), Q_ARG(QVariant, true));
        QTRY_VERIFY2_WITH_TIMEOUT(desk() && desk()->state() == "ready",
                                  desk() ? qPrintable(desk()->state() + " " + desk()->errorCode() + " " + desk()->errorText() + " daemon:" + be->desk()->daemonState() + be->desk()->daemonError()) : "no desk", 30000);
        QTRY_VERIFY_WITH_TIMEOUT(firstTerminal() && firstTerminal()->state() == "connected", 20000);
        TerminalSession *t = firstTerminal();
        QTest::qWait(800);
        t->sendInput("echo hello-$((6*7)); printf '\\033[31mred\\033[0m \\033[1;32mbold green\\033[0m \\033[38;5;208morange\\033[0m\\n'; ls --color=always /etc | head -12\r");
        QTRY_VERIFY_WITH_TIMEOUT(screen(t).contains("hello-42"), 10000);
        // кириллица и юникод через сервер
        t->sendInput("echo 'привет мир ✓ 日本'\r");
        QTRY_VERIFY_WITH_TIMEOUT(screen(t).contains("привет мир ✓ 日本"), 10000);
        shot("24-live-terminal");
    }
    void monitorWindow()
    {
        QMetaObject::invokeMethod(page(), "openWindow", Q_ARG(QVariant, "monitor"), Q_ARG(QVariant, QVariant()), Q_ARG(QVariant, QVariant()), Q_ARG(QVariant, QVariant()));
        QTRY_VERIFY_WITH_TIMEOUT(!desk()->monitor().value("cpu").toMap().isEmpty(), 20000);
        QTRY_VERIFY_WITH_TIMEOUT(desk()->cpuHistory().size() >= 3, 15000);
        QVERIFY(!desk()->monitor().value("processes").toList().isEmpty());
        shot("25-live-monitor");
    }
    void snapWindows()
    {
        const QVariantList wins = page()->property("wins").toList();
        QCOMPARE(wins.size(), 2);
        QObject *term = wins[0].toMap().value("win").value<QObject *>();
        QObject *mon = wins[1].toMap().value("win").value<QObject *>();
        QMetaObject::invokeMethod(term, "snapTo", Q_ARG(QVariant, "left"));
        QMetaObject::invokeMethod(mon, "snapTo", Q_ARG(QVariant, "right"));
        QTest::qWait(600);
        QCOMPARE(term->property("width").toDouble() * 2, page()->property("area").value<QObject *>()->property("width").toDouble());
        TerminalSession *t = firstTerminal();
        QVERIFY(t->emulator()->cols() < 100);   // терминал ужался вместе с окном
        shot("26-live-desktop-snapped");
        // свернуть и развернуть
        QMetaObject::invokeMethod(term, "minimize");
        QCOMPARE(term->property("minimized").toBool(), true);
        shot("27-live-minimized");
        QMetaObject::invokeMethod(term, "unminimize");
    }
    void launcher()
    {
        QObject *l = page()->findChild<QObject *>("launcher");
        QVERIFY(l);
        QMetaObject::invokeMethod(l, "open");
        shot("28-live-launcher");
        QMetaObject::invokeMethod(l, "close");
    }
    void clientReconnectReattachesSession()
    {
        TerminalSession *t = firstTerminal();
        const QString sid = t->sessionId();
        QVERIFY(!sid.isEmpty());
        t->sendInput("echo before-drop\r");
        QTRY_VERIFY_WITH_TIMEOUT(screen(t).contains("before-drop"), 10000);
        be->daemon()->dropConnectionForTest();
        QTRY_COMPARE_WITH_TIMEOUT(t->state(), QString("lost"), 5000);
        QCOMPARE(desk()->state(), QString("lost"));
        shot("29-live-lost");
        QTRY_COMPARE_WITH_TIMEOUT(t->state(), QString("connected"), 20000);
        QCOMPARE(t->sessionId(), sid);
        QTRY_COMPARE_WITH_TIMEOUT(desk()->state(), QString("ready"), 20000);
        t->sendInput("echo after-reattach-$((20+22))\r");
        QTRY_VERIFY_WITH_TIMEOUT(screen(t).contains("after-reattach-42"), 10000);
        // мониторинг переподписался
        const QString ts = desk()->monitor().value("timestamp").toString();
        QTRY_VERIFY_WITH_TIMEOUT(desk()->monitor().value("timestamp").toString() != ts, 15000);
    }
    void sessionExitAndReconnect()
    {
        TerminalSession *t = firstTerminal();
        t->sendInput("exit\r");
        QTRY_COMPARE_WITH_TIMEOUT(t->state(), QString("closed"), 10000);
        QCOMPARE(t->reason(), QString("exit"));
        shot("30-live-session-ended");
        t->reconnect();
        QTRY_COMPARE_WITH_TIMEOUT(t->state(), QString("connected"), 15000);
        t->sendInput("echo second-session\r");
        QTRY_VERIFY_WITH_TIMEOUT(screen(t).contains("second-session"), 10000);
    }
    void layoutPersists()
    {
        QTest::qWait(1000);   // debounce сохранения
        QVERIFY(QFile::exists(QStandardPaths::writableLocation(QStandardPaths::AppDataLocation) + "/desktops/" + okId + ".json"));
        QMetaObject::invokeMethod(win, "doCloseDesktop", Q_ARG(QVariant, okId));
        QTRY_VERIFY_WITH_TIMEOUT(be->desk()->desktops().isEmpty(), 5000);
        QMetaObject::invokeMethod(win, "openDesktop", Q_ARG(QVariant, okId), Q_ARG(QVariant, false));
        QTRY_VERIFY_WITH_TIMEOUT(desk() && desk()->state() == "ready", 30000);
        QTRY_COMPARE_WITH_TIMEOUT(windowCount(), 2, 10000);   // терминал и монитор восстановлены
        QTRY_VERIFY_WITH_TIMEOUT(firstTerminal() && firstTerminal()->state() == "connected", 20000);
        shot("31-live-layout-restored");
        be->setLanguage("ru");
        QTest::qWait(2800);   // дождаться очередного замера монитора
        shot("33-live-desktop-ru");
        be->setLanguage("en");
    }
    void authFailedDesktop()
    {
        QMetaObject::invokeMethod(win, "openDesktop", Q_ARG(QVariant, badId), Q_ARG(QVariant, false));
        DesktopState *d = be->desk()->desktop(badId);
        QVERIFY(d);
        QTRY_COMPARE_WITH_TIMEOUT(d->state(), QString("error"), 20000);
        QVERIFY(d->errorText().contains("rejected"));
        QTest::qWait(300);
        QObject *rep = prop("desktopRepeater");
        QQuickItem *p = nullptr;
        QMetaObject::invokeMethod(rep, "itemAt", Q_RETURN_ARG(QQuickItem *, p), Q_ARG(int, 1));
        QVERIFY(p);
        shot("32-live-desktop-auth-failed");
    }
    void cleanupTestCase()
    {
        if (be)
            be->client()->runBlocking({"daemon", "stop", "--force"});
        delete engine;
    }
};

QTEST_MAIN(TstLive)
#include "tst_live.moc"
