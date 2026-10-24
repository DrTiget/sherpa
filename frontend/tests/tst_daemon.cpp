// DaemonClient против настоящего демона: ensure, hello, методы, ping, обрыв и автопереподключение.
#include "CoreEnv.h"
#include "DaemonClient.h"

#include <QSignalSpy>
#include <QtTest>

class TstDaemon : public QObject {
    Q_OBJECT
    CoreEnv env;
    CoreClient core;
    DaemonClient *d = nullptr;

private slots:
    void initTestCase()
    {
        QVERIFY2(!qEnvironmentVariableIsEmpty("SHERPA_CORE"), "set SHERPA_CORE");
        qRegisterMetaType<DaemonClient::State>();
        qputenv("XDG_RUNTIME_DIR", env.dir.filePath("run").toUtf8());
        QDir().mkpath(env.dir.filePath("run"));
        QFile(env.dir.filePath("run")).setPermissions(QFile::ReadOwner | QFile::WriteOwner | QFile::ExeOwner);
        QVERIFY(core.runBlocking({"init"}).ok);
        d = new DaemonClient(&core, this);
    }
    void startsDaemonAndHandshakes()
    {
        QSignalSpy st(d, &DaemonClient::stateChanged);
        d->start();
        QTRY_VERIFY_WITH_TIMEOUT(d->isReady(), 15000);
        QVERIFY(!d->endpoint().isEmpty());
        QVERIFY(d->hasMethod("session.open"));
        QVERIFY(d->hasMethod("monitor.subscribe"));
    }
    void requestResponse()
    {
        bool done = false;
        d->request("ping", {}, [&](const DaemonReply &r) {
            QVERIFY(r.ok);
            QVERIFY(r.data.toMap().value("pong").toBool());
            done = true;
        });
        QTRY_VERIFY_WITH_TIMEOUT(done, 3000);
    }
    void errorReply()
    {
        bool done = false;
        d->request("session.input", {{"session", "nope"}, {"data_b64", "AA=="}}, [&](const DaemonReply &r) {
            QVERIFY(!r.ok);
            QCOMPARE(r.error.code, QString("session.not_found"));
            done = true;
        });
        QTRY_VERIFY_WITH_TIMEOUT(done, 3000);
    }
    void parallelRequestsMatchedById()
    {
        int ok = 0;
        for (int i = 0; i < 20; ++i)
            d->request("ping", {}, [&](const DaemonReply &r) { if (r.ok) ++ok; });
        QTRY_COMPARE_WITH_TIMEOUT(ok, 20, 5000);
    }
    void dropReconnects()
    {
        QSignalSpy lost(d, &DaemonClient::connectionLost);
        QSignalSpy back(d, &DaemonClient::reconnected);
        bool pendingFailed = false;
        d->dropConnectionForTest();
        QCOMPARE(lost.size(), 1);
        d->request("ping", {}, [&](const DaemonReply &r) { pendingFailed = !r.ok && r.error.code == "client.daemon_unavailable"; });
        QTRY_VERIFY_WITH_TIMEOUT(pendingFailed, 1000);
        QTRY_COMPARE_WITH_TIMEOUT(back.size(), 1, 20000);
        QVERIFY(d->isReady());
    }
    void daemonStopFailsPendingAndReports()
    {
        d->setAutoReconnect(false);
        QSignalSpy lost(d, &DaemonClient::connectionLost);
        d->request("daemon.stop", {{"force", true}});
        QTRY_COMPARE_WITH_TIMEOUT(lost.size(), 1, 5000);
        QVERIFY(!d->isReady());
    }
    void cleanupTestCase() { core.runBlocking({"daemon", "stop", "--force"}); }
};

QTEST_MAIN(TstDaemon)
#include "tst_daemon.moc"
