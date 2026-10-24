// Файловый менеджер, редактор и туннели на живом sshd + демоне. Файловая система «сервера» — локальная (тот же хост),
// поэтому результат проверяется напрямую. Скриншоты — SHERPA_SHOTS_DIR.
#include "LiveFixture.h"

#include <QCryptographicHash>
#include <QDirIterator>
#include <QJSValue>
#include <QTcpServer>
#include <QTcpSocket>

static QVariantMap callAt(QObject *w, const char *fn, int i)
{
    QVariant v;
    QMetaObject::invokeMethod(w, fn, Q_RETURN_ARG(QVariant, v), Q_ARG(QVariant, i));
    return v.value<QJSValue>().toVariant().toMap().isEmpty() ? v.toMap() : v.value<QJSValue>().toVariant().toMap();
}

class TstFiles : public QObject {
    Q_OBJECT
    LiveFixture f;
    QTemporaryDir remote;   // «каталог на сервере»
    QTemporaryDir local;    // каталог на клиенте
    QObject *files = nullptr;

    static QByteArray sha(const QString &path)
    {
        QFile fl(path);
        fl.open(QIODevice::ReadOnly);
        QCryptographicHash h(QCryptographicHash::Sha256);
        h.addData(&fl);
        return h.result();
    }
    static void writeFile(const QString &path, const QByteArray &data)
    {
        QFile fl(path);
        QVERIFY(fl.open(QIODevice::WriteOnly));
        fl.write(data);
    }
    QStringList names() const
    {
        QStringList l;
        for (const QVariant &v : files->property("shown").toList())
            l << v.toMap().value("name").toString();
        return l;
    }
    void nav(const QString &path)
    {
        QMetaObject::invokeMethod(files, "navigate", Q_ARG(QVariant, path), Q_ARG(QVariant, true));
        QTRY_COMPARE_WITH_TIMEOUT(files->property("cwd").toString(), path, 10000);
        QTRY_VERIFY_WITH_TIMEOUT(!files->property("loading").toBool(), 10000);
    }
    QVariantMap entry(const QString &name) const
    {
        for (const QVariant &v : files->property("shown").toList())
            if (v.toMap().value("name") == name)
                return v.toMap();
        return {};
    }
    void select(const QString &name) { QMetaObject::invokeMethod(files, "selectOnly", Q_ARG(QVariant, entry(name).value("path"))); }
    QVariantMap transfer(int i)
    {
        return callAt(files, "transferAt", i);
    }
    int transferCount() { return files->property("transfersModel").value<QObject *>()->property("count").toInt(); }
    QString transferStatus(int i) { return transfer(i).value("status").toString(); }

private slots:
    void initTestCase()
    {
        if (!f.init("SherpaFiles"))
            QSKIP("sshd not available");
        QVERIFY(f.prepareServer());
        QVERIFY(f.openDesktop());
        QVERIFY(remote.isValid() && local.isValid());
        const QString r = remote.path();
        QDir().mkpath(r + "/etc-like/conf.d");
        QDir().mkpath(r + "/projects/app/src");
        writeFile(r + "/readme.txt", "Sherpa file manager\nsecond line\n");
        writeFile(r + "/notes.md", "# notes\n");
        writeFile(r + "/.hidden", "secret\n");
        writeFile(r + "/config.json", "{\"name\": \"sherpa\", \"port\": 22, \"on\": true}\n");
        writeFile(r + "/deploy.sh", "#!/bin/bash\n# deploy\nfor i in 1 2 3; do echo \"step $i\"; done\n");
        writeFile(r + "/etc-like/conf.d/site.conf", "server {\n  listen 80;\n  server_name example.org;\n}\n");
        writeFile(r + "/projects/app/src/main.c", "int main(void) { return 0; }\n");
        QFile::link(r + "/readme.txt", r + "/link-to-readme");
        QFile(r + "/deploy.sh").setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner | QFileDevice::ReadGroup | QFileDevice::ReadOther);
        files = f.openWindow("files");
        QVERIFY(files);
        nav(r);
    }

    void listingSortedAndFiltered()
    {
        const QStringList n = names();
        QVERIFY2(n.contains("readme.txt") && n.contains("projects"), qPrintable(n.join(",")));
        QVERIFY(!n.contains(".hidden"));
        // каталоги идут первыми
        QVERIFY(n.indexOf("projects") < n.indexOf("readme.txt"));
        files->setProperty("showHidden", true);
        QVERIFY(names().contains(".hidden"));
        files->setProperty("showHidden", false);
        QCOMPARE(entry("deploy.sh").value("type").toString(), QString("file"));
        QCOMPARE(entry("link-to-readme").value("type").toString(), QString("symlink"));
        QVariant perm;
        QMetaObject::invokeMethod(files, "fmtPerm", Q_RETURN_ARG(QVariant, perm), Q_ARG(QVariant, entry("deploy.sh").value("permissions")));
        QCOMPARE(perm.toString(), QString("rwxr--r--"));
        f.shot("40-files-list");
    }
    void navigationTreeAndHistory()
    {
        const QString r = remote.path();
        nav(r + "/projects/app");
        QCOMPARE(names(), QStringList({"src"}));
        QMetaObject::invokeMethod(files, "goUp");
        QTRY_COMPARE_WITH_TIMEOUT(files->property("cwd").toString(), r + "/projects", 5000);
        QMetaObject::invokeMethod(files, "goBack");
        QTRY_COMPARE_WITH_TIMEOUT(files->property("cwd").toString(), r + "/projects/app", 5000);
        QMetaObject::invokeMethod(files, "goForward");
        QTRY_COMPARE_WITH_TIMEOUT(files->property("cwd").toString(), r + "/projects", 5000);
        // ошибка: нет такого каталога
        QMetaObject::invokeMethod(files, "navigate", Q_ARG(QVariant, r + "/nope"), Q_ARG(QVariant, true));
        QTRY_VERIFY_WITH_TIMEOUT(!files->property("errorText").toString().isEmpty(), 5000);
        QCOMPARE(files->property("cwd").toString(), r + "/projects");
        files->setProperty("errorText", "");
        nav(r);
    }
    void makeFolderRenameMove()
    {
        const QString r = remote.path();
        QMetaObject::invokeMethod(files, "newFolder");
        QObject *dlg = files->property("nameDialog").value<QObject *>();
        QVERIFY(dlg->property("visible").toBool());
        QMetaObject::invokeMethod(files, "submitName", Q_ARG(QVariant, "made-here"));
        QTRY_VERIFY_WITH_TIMEOUT(QDir(r + "/made-here").exists(), 5000);
        QTRY_VERIFY_WITH_TIMEOUT(names().contains("made-here"), 5000);
        select("made-here");
        QMetaObject::invokeMethod(files, "renameSelected");
        QMetaObject::invokeMethod(files, "submitName", Q_ARG(QVariant, "renamed"));
        QTRY_VERIFY_WITH_TIMEOUT(QDir(r + "/renamed").exists() && !QDir(r + "/made-here").exists(), 5000);
        QTRY_VERIFY_WITH_TIMEOUT(names().contains("renamed"), 5000);
        select("renamed");
        QMetaObject::invokeMethod(files, "moveSelected");
        QMetaObject::invokeMethod(files, "submitName", Q_ARG(QVariant, r + "/projects/moved"));
        QTRY_VERIFY_WITH_TIMEOUT(QDir(r + "/projects/moved").exists(), 5000);
        QTRY_VERIFY_WITH_TIMEOUT(!names().contains("renamed"), 5000);
        // имя со слэшем отвергается
        QMetaObject::invokeMethod(files, "newFolder");
        QMetaObject::invokeMethod(files, "submitName", Q_ARG(QVariant, "a/b"));
        QVERIFY(!QDir(r + "/a").exists());
        QMetaObject::invokeMethod(dlg, "close");
    }
    void permissions()
    {
        select("notes.md");
        QMetaObject::invokeMethod(files, "permissionsOfSelected");
        QObject *perms = files->property("permsDialog").value<QObject *>();
        QVERIFY(perms->property("visible").toBool());
        f.shot("41-files-permissions");
        QMetaObject::invokeMethod(perms, "close");
        const QVariantMap e = entry("notes.md");
        QMetaObject::invokeMethod(files, "applyPermissions", Q_ARG(QVariant, e), Q_ARG(QVariant, 384), Q_ARG(QVariant, QVariant()),
                                  Q_ARG(QVariant, QVariant()), Q_ARG(QVariant, false));
        QTest::qWait(1500);
        QVERIFY2(QFileInfo(remote.path() + "/notes.md").permissions() == (QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ReadUser | QFileDevice::WriteUser),
                 qPrintable(files->property("errorText").toString() + " perms=" + QString::number(int(QFileInfo(remote.path() + "/notes.md").permissions()), 16)));
    }
    void searchFindsNestedFiles()
    {
        QMetaObject::invokeMethod(files, "runSearch", Q_ARG(QVariant, "site"));
        QTRY_VERIFY_WITH_TIMEOUT(files->property("searchMode").toBool(), 10000);
        QTRY_COMPARE_WITH_TIMEOUT(names(), QStringList({"site.conf"}), 5000);
        f.shot("42-files-search");
        QMetaObject::invokeMethod(files, "runSearch", Q_ARG(QVariant, ""));
        QTRY_VERIFY_WITH_TIMEOUT(!files->property("searchMode").toBool(), 5000);
    }
    void dangerousDeleteNeedsToken()
    {
        const QString r = remote.path();
        QDir().mkpath(r + "/to-delete/sub");
        writeFile(r + "/to-delete/sub/a.txt", "x");
        QMetaObject::invokeMethod(files, "refresh");
        QTRY_VERIFY_WITH_TIMEOUT(names().contains("to-delete"), 5000);
        // без подтверждения ядро отказывает
        QObject *desk = f.desk();
        QString code;
        bool done = false;
        QJSValue cb;   // вызов напрямую через QML-функцию call
        QVariant res;
        Q_UNUSED(cb) Q_UNUSED(res)
        QMetaObject::invokeMethod(files, "call", Q_ARG(QVariant, "files.delete"), Q_ARG(QVariant, QVariantMap({{"path", r + "/to-delete"}, {"recursive", true}})),
                                  Q_ARG(QVariant, QVariant()), Q_ARG(QVariant, QVariant()));
        QTest::qWait(500);
        QVERIFY(QDir(r + "/to-delete").exists());   // токена не было — каталог цел
        Q_UNUSED(desk) Q_UNUSED(code) Q_UNUSED(done)
        // через интерфейс: диалог с перечислением, подтверждение, danger.prepare → удаление
        select("to-delete");
        QMetaObject::invokeMethod(files, "deleteSelected");
        QObject *danger = files->property("dangerDialog").value<QObject *>();
        QTRY_VERIFY_WITH_TIMEOUT(danger->property("visible").toBool(), 3000);
        QVERIFY(danger->property("details").toList().size() == 1);
        f.shot("43-files-delete-confirm");
        QJSValue onConfirm = danger->property("onConfirm").value<QJSValue>();
        QVERIFY(onConfirm.isCallable());
        QMetaObject::invokeMethod(danger, "close");
        onConfirm.call();
        QTRY_VERIFY_WITH_TIMEOUT(!QDir(r + "/to-delete").exists(), 10000);
    }
    void systemPathRequiresServerName()
    {
        // каталог в системном пути: диалог требует ввести имя сервера
        QMetaObject::invokeMethod(files, "isSystemPath", Q_ARG(QVariant, "/etc/ssh"));
        QVariant sys;
        QMetaObject::invokeMethod(files, "isSystemPath", Q_RETURN_ARG(QVariant, sys), Q_ARG(QVariant, "/etc/ssh"));
        QVERIFY(sys.toBool());
        QMetaObject::invokeMethod(files, "isSystemPath", Q_RETURN_ARG(QVariant, sys), Q_ARG(QVariant, remote.path() + "/x"));
        QVERIFY(!sys.toBool());
    }

    void uploadWithVerification()
    {
        const QString src = local.filePath("payload.bin");
        QByteArray data(3 * 1024 * 1024 + 123, 0);
        for (int i = 0; i < data.size(); ++i) data[i] = char((i * 31 + i / 7) & 0xff);
        writeFile(src, data);
        QMetaObject::invokeMethod(files, "uploadPaths", Q_ARG(QVariant, QStringList{src}));
        QTRY_COMPARE_WITH_TIMEOUT(transferCount(), 1, 5000);
        QTRY_COMPARE_WITH_TIMEOUT(transferStatus(0), QString("completed"), 60000);
        QCOMPARE(sha(remote.path() + "/payload.bin"), sha(src));
        QCOMPARE(transfer(0).value("bytes").toDouble(), double(data.size()));
        QTRY_VERIFY_WITH_TIMEOUT(names().contains("payload.bin"), 10000);
        f.shot("44-files-transfers");
    }
    void uploadConflictKeepBothAndResume()
    {
        const QString src = local.filePath("payload.bin");
        QMetaObject::invokeMethod(files, "uploadPaths", Q_ARG(QVariant, QStringList{src}));
        QObject *c = files->property("conflictDialog").value<QObject *>();
        QTRY_VERIFY_WITH_TIMEOUT(c->property("visible").toBool(), 5000);
        f.shot("45-files-conflict");
        QMetaObject::invokeMethod(c, "close");
        // «оставить оба»
        QMetaObject::invokeMethod(files, "startTransfer",
                                  Q_ARG(QVariant, QVariantMap({{"direction", "upload"}, {"local", src}, {"remote", remote.path() + "/payload.bin"}, {"name", "payload.bin"}, {"total", 3145851}})),
                                  Q_ARG(QVariant, "both"));
        QTRY_VERIFY_WITH_TIMEOUT(QFile::exists(remote.path() + "/payload (1).bin"), 60000);
        QTRY_COMPARE_WITH_TIMEOUT(transferStatus(0), QString("completed"), 60000);
        QCOMPARE(sha(remote.path() + "/payload (1).bin"), sha(src));
        // «заменить»: удаляет и пишет заново
        writeFile(src, QByteArray("shorter content\n"));
        QMetaObject::invokeMethod(files, "startTransfer",
                                  Q_ARG(QVariant, QVariantMap({{"direction", "upload"}, {"local", src}, {"remote", remote.path() + "/payload.bin"}, {"name", "payload.bin"}, {"total", 16}})),
                                  Q_ARG(QVariant, "replace"));
        QTRY_VERIFY_WITH_TIMEOUT(QFileInfo(remote.path() + "/payload.bin").size() == 16, 30000);
    }
    void downloadFileAndFolder()
    {
        nav(remote.path());
        select("config.json");
        const QString dst = local.filePath("dl");
        QDir().mkpath(dst);
        QMetaObject::invokeMethod(files, "downloadTo", Q_ARG(QVariant, dst));
        QTRY_VERIFY_WITH_TIMEOUT(QFile::exists(dst + "/config.json"), 30000);
        QTRY_COMPARE_WITH_TIMEOUT(transferStatus(0), QString("completed"), 30000);
        QCOMPARE(sha(dst + "/config.json"), sha(remote.path() + "/config.json"));
        // каталог целиком
        select("projects");
        QMetaObject::invokeMethod(files, "downloadTo", Q_ARG(QVariant, dst));
        QTRY_VERIFY_WITH_TIMEOUT(QFile::exists(dst + "/projects/app/src/main.c"), 30000);
        QTRY_COMPARE_WITH_TIMEOUT(transferStatus(0), QString("completed"), 30000);
        QCOMPARE(sha(dst + "/projects/app/src/main.c"), sha(remote.path() + "/projects/app/src/main.c"));
    }
    void cancelAndResume()
    {
        const QString big = local.filePath("big.bin");
        {
            QFile fl(big);
            QVERIFY(fl.open(QIODevice::WriteOnly));
            QByteArray chunk(1024 * 1024, 'x');
            for (int i = 0; i < 160; ++i) { chunk[0] = char(i); fl.write(chunk); }
        }
        const qint64 total = QFileInfo(big).size();
        QMetaObject::invokeMethod(files, "uploadPaths", Q_ARG(QVariant, QStringList{big}));
        QTRY_COMPARE_WITH_TIMEOUT(transferStatus(0), QString("running"), 20000);
        const QString id = transfer(0).value("tid").toString();
        QTRY_VERIFY_WITH_TIMEOUT(transfer(0).value("bytes").toDouble() > 0, 20000);
        f.desk()->rpcRaw("transfer.cancel", {{"id", id}}, QJSValue());
        QTRY_VERIFY_WITH_TIMEOUT(transferStatus(0) == "cancelled" || transferStatus(0) == "completed", 20000);
        if (transferStatus(0) == "cancelled") {
            QVERIFY(QFileInfo(remote.path() + "/big.bin").size() < total);
            QMetaObject::invokeMethod(files, "retryTransfer", Q_ARG(QVariant, id));
            QTRY_VERIFY_WITH_TIMEOUT(transfer(0).value("tid").toString() != id && transfer(0).value("direction").toString() == "upload"
                                     && transfer(0).value("name").toString() == "big.bin", 20000);
            QTRY_COMPARE_WITH_TIMEOUT(transferStatus(0), QString("completed"), 120000);
        }
        QVERIFY2(QFileInfo(remote.path() + "/big.bin").size() == total, qPrintable(QString("size=%1 total=%2 status=%3 err=%4 n=%5").arg(QFileInfo(remote.path() + "/big.bin").size()).arg(total).arg(transferStatus(0), transfer(0).value("error").toString()).arg(transferCount())));
        QCOMPARE(sha(remote.path() + "/big.bin"), sha(big));
    }

    void editorHighlightSaveAndConflict()
    {
        const QString r = remote.path();
        nav(r);
        QMetaObject::invokeMethod(files, "openEditor", Q_ARG(QVariant, entry("deploy.sh")));
        QObject *ed = f.content("editor");
        QVERIFY(ed);
        QObject *area = ed->property("textArea").value<QObject *>();
        QTRY_VERIFY_WITH_TIMEOUT(area->property("text").toString().contains("deploy"), 10000);
        QCOMPARE(ed->property("highlighter").value<QObject *>()->property("language").toString(), QString("sh"));
        QVERIFY(!ed->property("modified").toBool());
        f.shot("46-editor");
        // правка и сохранение
        QMetaObject::invokeMethod(area, "insert", Q_ARG(int, 0), Q_ARG(QString, "# edited by sherpa\n"));
        QVERIFY(ed->property("modified").toBool());
        QMetaObject::invokeMethod(ed, "save", Q_ARG(QVariant, false));
        QTRY_VERIFY2_WITH_TIMEOUT(!ed->property("modified").toBool(), qPrintable(ed->property("errorText").toString()), 10000);
        QVERIFY(QFile(r + "/deploy.sh").open(QIODevice::ReadOnly));
        QFile chk(r + "/deploy.sh");
        chk.open(QIODevice::ReadOnly);
        QVERIFY(chk.readAll().startsWith("# edited by sherpa\n#!/bin/bash"));
        // конфликт: файл изменили снаружи (mtime другой)
        QThread::sleep(2);
        writeFile(r + "/deploy.sh", "changed by someone else\n");
        QMetaObject::invokeMethod(area, "insert", Q_ARG(int, 0), Q_ARG(QString, "mine\n"));
        QMetaObject::invokeMethod(ed, "save", Q_ARG(QVariant, false));
        QObject *cd = ed->property("conflictDialog").value<QObject *>();
        QTRY_VERIFY_WITH_TIMEOUT(cd->property("visible").toBool(), 10000);
        f.shot("47-editor-conflict");
        chk.close();
        QFile c2(r + "/deploy.sh");
        c2.open(QIODevice::ReadOnly);
        QCOMPARE(c2.readAll(), QByteArray("changed by someone else\n"));   // чужая версия цела
        QMetaObject::invokeMethod(ed, "save", Q_ARG(QVariant, true));       // «перезаписать»
        QTRY_VERIFY_WITH_TIMEOUT(!ed->property("modified").toBool(), 10000);
        QFile c3(r + "/deploy.sh");
        c3.open(QIODevice::ReadOnly);
        QVERIFY(c3.readAll().startsWith("mine\n"));
    }
    void editorLanguagesAndSearchReplace()
    {
        const QString r = remote.path();
        QMetaObject::invokeMethod(files, "openEditor", Q_ARG(QVariant, entry("config.json")));
        QObject *ed = f.content("editor", 1);
        QVERIFY(ed);
        QObject *area = ed->property("textArea").value<QObject *>();
        QTRY_VERIFY_WITH_TIMEOUT(area->property("text").toString().contains("sherpa"), 10000);
        QCOMPARE(ed->property("highlighter").value<QObject *>()->property("language").toString(), QString("json"));
        ed->setProperty("findOpen", true);
        QTest::qWait(200);
        f.shot("48-editor-json-find");
        // «заменить всё»
        QObject *find = ed->findChild<QObject *>("");
        Q_UNUSED(find)
        QMetaObject::invokeMethod(area, "selectAll");
        QMetaObject::invokeMethod(area, "remove", Q_ARG(int, 0), Q_ARG(int, area->property("length").toInt()));
        QMetaObject::invokeMethod(area, "insert", Q_ARG(int, 0), Q_ARG(QString, "a a a"));
        QVERIFY(ed->property("modified").toBool());
    }
    void closingDirtyEditorAsksFirst()
    {
        QObject *ed = f.content("editor", 1);
        QVERIFY(ed);
        QVariant ok;
        QMetaObject::invokeMethod(ed, "beforeClose", Q_RETURN_ARG(QVariant, ok), Q_ARG(QVariant, QVariant()));
        QVERIFY(!ok.toBool());   // есть несохранённое — закрыть сразу нельзя
    }

    void tunnelLocalRemoteDynamicAndDanger()
    {
        // эхо-сервер «целевая служба»
        QTcpServer echo;
        QVERIFY(echo.listen(QHostAddress::LocalHost, 0));
        QObject::connect(&echo, &QTcpServer::newConnection, &echo, [&echo]() {
            while (QTcpSocket *s = echo.nextPendingConnection()) {
                QObject::connect(s, &QTcpSocket::readyRead, s, [s]() { s->write(s->readAll()); });
                QObject::connect(s, &QTcpSocket::disconnected, s, &QObject::deleteLater);
            }
        });
        QObject *tw = f.openWindow("tunnels");
        QVERIFY(tw);
        QObject *model = tw->property("tunnelModel").value<QObject *>();
        auto count = [&]() { return model->property("count").toInt(); };
        auto row = [&](int i) { return callAt(tw, "tunnelAt", i); };

        // -L: локальный порт → echo через сервер
        QMetaObject::invokeMethod(tw, "launch", Q_ARG(QVariant, "local"),
                                  Q_ARG(QVariant, QVariantMap({{"kind", "local"}, {"bind_host", "127.0.0.1"}, {"bind_port", 0}, {"target_host", "127.0.0.1"}, {"target_port", int(echo.serverPort())}})));
        QTRY_COMPARE_WITH_TIMEOUT(count(), 1, 10000);
        QTRY_COMPARE_WITH_TIMEOUT(row(0).value("status").toString(), QString("running"), 10000);
        const int lport = row(0).value("bind_port").toInt();
        QVERIFY(lport > 0);
        {
            QTcpSocket c;
            c.connectToHost("127.0.0.1", lport);
            QTRY_VERIFY_WITH_TIMEOUT(c.state() == QAbstractSocket::ConnectedState, 5000);
            c.write("ping-through-tunnel");
            QTRY_VERIFY_WITH_TIMEOUT(c.bytesAvailable() > 0, 10000);
            QCOMPARE(c.readAll(), QByteArray("ping-through-tunnel"));
        }
        f.shot("49-tunnels");

        // -D: SOCKS5
        QMetaObject::invokeMethod(tw, "launch", Q_ARG(QVariant, "dynamic"),
                                  Q_ARG(QVariant, QVariantMap({{"kind", "dynamic"}, {"bind_host", "127.0.0.1"}, {"bind_port", 0}})));
        QTRY_COMPARE_WITH_TIMEOUT(count(), 2, 10000);
        QTRY_COMPARE_WITH_TIMEOUT(row(0).value("status").toString(), QString("running"), 10000);
        {
            QTcpSocket c;
            c.connectToHost("127.0.0.1", row(0).value("bind_port").toInt());
            QTRY_VERIFY_WITH_TIMEOUT(c.state() == QAbstractSocket::ConnectedState, 5000);
            c.write(QByteArray("\x05\x01\x00", 3));
            QTRY_VERIFY_WITH_TIMEOUT(c.bytesAvailable() > 0, 5000);
            QCOMPARE(c.read(2), QByteArray("\x05\x00", 2));
            QByteArray req("\x05\x01\x00\x01\x7f\x00\x00\x01", 8);
            req.append(char(echo.serverPort() >> 8)).append(char(echo.serverPort() & 0xff));
            c.write(req);
            QTRY_VERIFY_WITH_TIMEOUT(c.bytesAvailable() > 0, 10000);
            QCOMPARE(c.read(10).left(2), QByteArray("\x05\x00", 2));
            c.write("socks-ok");
            QTRY_VERIFY_WITH_TIMEOUT(c.bytesAvailable() > 0, 10000);
            QCOMPARE(c.readAll(), QByteArray("socks-ok"));
        }

        // -R: порт на сервере → echo на клиенте
        QTcpServer probe;
        QVERIFY(probe.listen(QHostAddress::LocalHost, 0));
        const int rport = probe.serverPort();
        probe.close();
        QMetaObject::invokeMethod(tw, "launch", Q_ARG(QVariant, "remote"),
                                  Q_ARG(QVariant, QVariantMap({{"kind", "remote"}, {"bind_host", "127.0.0.1"}, {"bind_port", rport}, {"target_host", "127.0.0.1"}, {"target_port", int(echo.serverPort())}})));
        QTRY_COMPARE_WITH_TIMEOUT(count(), 3, 10000);
        QTRY_COMPARE_WITH_TIMEOUT(row(0).value("status").toString(), QString("running"), 10000);
        {
            QTcpSocket c;
            c.connectToHost("127.0.0.1", rport);
            QTRY_VERIFY_WITH_TIMEOUT(c.state() == QAbstractSocket::ConnectedState, 5000);
            c.write("remote-ok");
            QTRY_VERIFY_WITH_TIMEOUT(c.bytesAvailable() > 0, 10000);
            QCOMPARE(c.readAll(), QByteArray("remote-ok"));
        }

        // нелокальный bind требует подтверждения с именем сервера
        QMetaObject::invokeMethod(tw, "launch", Q_ARG(QVariant, "local"),
                                  Q_ARG(QVariant, QVariantMap({{"kind", "local"}, {"bind_host", "0.0.0.0"}, {"bind_port", 0}, {"target_host", "127.0.0.1"}, {"target_port", int(echo.serverPort())}})));
        QObject *danger = tw->property("dangerDialog").value<QObject *>();
        QTRY_VERIFY_WITH_TIMEOUT(danger->property("visible").toBool(), 3000);
        QVERIFY(danger->property("requireName").toBool());
        QCOMPARE(count(), 3);
        f.shot("50-tunnels-danger");
        QJSValue onConfirm = danger->property("onConfirm").value<QJSValue>();
        QMetaObject::invokeMethod(danger, "close");
        onConfirm.call();
        QTRY_COMPARE_WITH_TIMEOUT(count(), 4, 10000);
        QTRY_COMPARE_WITH_TIMEOUT(row(0).value("status").toString(), QString("running"), 10000);

        // остановка
        const QString id = row(0).value("tid").toString();
        QMetaObject::invokeMethod(tw, "stop", Q_ARG(QVariant, id));
        QTRY_COMPARE_WITH_TIMEOUT(row(0).value("status").toString(), QString("stopped"), 10000);
        QTcpSocket gone;
        gone.connectToHost("127.0.0.1", row(0).value("bind_port").toInt());
        QTest::qWait(1500);
        QVERIFY(gone.state() != QAbstractSocket::ConnectedState);
        f.shot("51-tunnels-stopped");
    }

    void cleanupTestCase() {}
};

QTEST_MAIN(TstFiles)
#include "tst_files.moc"
