// Тесты CoreClient против настоящего sherpa-core с временной БД (SHERPA_CORE указывает бинарник).
#include "CoreClient.h"
#include "CoreEnv.h"
#include "ErrorText.h"

#include <QJsonArray>
#include <QSignalSpy>
#include <QtTest>

class TstCoreClient : public QObject {
    Q_OBJECT
    CoreEnv env;
    CoreClient c;
    QString connId;

private slots:
    void initTestCase()
    {
        QVERIFY2(!qEnvironmentVariableIsEmpty("SHERPA_CORE"), "set SHERPA_CORE to the sherpa-core binary");
        qRegisterMetaType<CoreResponse>();
    }
    void notInitialized()
    {
        CoreClient fresh;
        fresh.setGlobalArgs({"--db", "/nonexistent-dir/x.db", "--key-file", "/nonexistent-dir/x.key"});
        auto r = fresh.runBlocking({"conn", "list"});
        QVERIFY(!r.ok);
        QCOMPARE(r.error.code, QString("store.not_initialized"));
        QCOMPARE(r.exitCode, 3);
    }
    void version()
    {
        auto r = c.runBlocking({"version", "--require-protocol", "1"});
        QVERIFY(r.ok);
        r = c.runBlocking({"version", "--require-protocol", "99"});
        QCOMPARE(r.error.code, QString("protocol.unsupported"));
        QCOMPARE(r.exitCode, 4);
    }
    void init()
    {
        auto r = c.runBlocking({"init"});
        QVERIFY(r.ok);
        QVERIFY(r.data.value("backup_recommended").toBool());
    }
    void createWithSecretViaStdin()
    {
        QJsonObject in{{"name", "web-1"}, {"host", "127.0.0.1"}, {"port", 1}, {"user", "root"},
                       {"tags", QJsonArray{"prod"}},
                       {"auth", QJsonObject{{"method", "password"}, {"password", "s3cr3t"}}}};
        auto r = c.runBlocking({"conn", "create"}, in, true);
        QVERIFY2(r.ok, qPrintable(r.error.code));
        const QVariantMap conn = r.data.value("connection").toMap();
        connId = conn.value("id").toString();
        QVERIFY(conn.value("auth").toMap().value("has_password").toBool());
        QVERIFY(!QString::fromUtf8(QJsonDocument::fromVariant(conn).toJson()).contains("s3cr3t"));
    }
    void duplicateNameIsError()
    {
        QJsonObject in{{"name", "WEB-1"}, {"host", "h"}, {"auth", QJsonObject{{"method", "password"}}}};
        auto r = c.runBlocking({"conn", "create"}, in, true);
        QVERIFY(!r.ok);
        QCOMPARE(r.error.code, QString("conn.name_taken"));
        QCOMPARE(ErrorText::field(r.error.code, r.error.params), QString("name"));
        QVERIFY(ErrorText::text(r.error.code, r.error.params).contains("WEB-1"));
    }
    void validationField()
    {
        QJsonObject in{{"name", "x"}, {"host", "h"}, {"port", 70000}, {"auth", QJsonObject{{"method", "password"}}}};
        auto r = c.runBlocking({"conn", "create"}, in, true);
        QVERIFY(!r.ok);
        QCOMPARE(ErrorText::field(r.error.code, r.error.params), QString("port"));
    }
    void list()
    {
        auto r = c.runBlocking({"conn", "list", "--q", "web"});
        QVERIFY(r.ok);
        QCOMPARE(r.data.value("items").toList().size(), 1);
    }
    void checkStreamDeliversEventsThenSummary()
    {
        QSignalSpy ev(&c, &CoreClient::streamEvent);
        QSignalSpy fin(&c, &CoreClient::finished);
        c.runStream({"conn", "check", connId, "--stream", "--timeout", "2"});
        QVERIFY(fin.wait(15000));
        QCOMPARE(ev.size(), 1);
        const QVariantMap item = ev.first().at(1).toMap().value("item").toMap();
        QCOMPARE(item.value("state").toString(), QString("offline"));
        QCOMPARE(item.value("error").toMap().value("code").toString(), QString("net.refused"));
        const auto resp = fin.first().at(1).value<CoreResponse>();
        QVERIFY(resp.ok);
        QCOMPARE(resp.data.value("summary").toMap().value("total").toInt(), 1);
    }
    void cancelKillsProcess()
    {
        // таймаут 2 с на недоступный адрес-«чёрную дыру»: отменяем раньше
        QJsonObject in{{"name", "hole"}, {"host", "10.255.255.1"}, {"auth", QJsonObject{{"method", "password"}}}};
        auto cr = c.runBlocking({"conn", "create"}, in, true);
        QVERIFY(cr.ok);
        QSignalSpy fin(&c, &CoreClient::finished);
        const quint64 id = c.runStream({"conn", "check", cr.data.value("connection").toMap().value("id").toString(),
                                        "--stream", "--timeout", "30"});
        QTest::qWait(300);
        c.cancel(id);
        QVERIFY(fin.wait(5000));
        QCOMPARE(fin.first().at(1).value<CoreResponse>().error.code, QString("client.cancelled"));
    }
    void missingBinary()
    {
        CoreClient bad;
        bad.setProgram("/nonexistent/sherpa-core");
        auto r = bad.runBlocking({"version"});
        QVERIFY(!r.ok);
        QCOMPARE(r.error.code, QString("client.core_not_found"));
    }
    void deleteAndNotFound()
    {
        auto r = c.runBlocking({"conn", "delete", connId});
        QVERIFY(r.ok);
        r = c.runBlocking({"conn", "get", connId});
        QCOMPARE(r.error.code, QString("conn.not_found"));
    }
    void errorTextUnknownCodeFallsBack()
    {
        const QString t = ErrorText::text("future.thing", {}, "boom");
        QVERIFY(t.contains("future.thing") && t.contains("boom"));
    }
    void keyExport()
    {
        const QString to = env.dir.filePath("copy.key");
        auto r = c.runBlocking({"keyfile", "export", "--to", to});
        QVERIFY(r.ok);
        r = c.runBlocking({"keyfile", "export", "--to", to});
        QCOMPARE(r.error.code, QString("fs.exists"));
    }
};

QTEST_MAIN(TstCoreClient)
#include "tst_coreclient.moc"
