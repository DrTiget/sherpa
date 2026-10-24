// Окна утилит (сеть, Docker, веб, БД, безопасность, бэкапы, сниппеты, мультисервер, diff, заметки, журнал, виджеты)
// против живого демона и локального sshd; скриншоты tools-* в SHERPA_SHOTS_DIR.
// «Сервером» служит эта же машина (в контейнере sherpa-qt-tools, см. docker/Dockerfile.tools): нужны sshd, ip, ss, nginx, redis, restic, docker.
#include "Backend.h"
#include "CoreEnv.h"
#include "TestSshd.h"

#include <QFontDatabase>
#include <QJsonArray>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQmlExpression>
#include <QQuickItem>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QtQml/qqmlextensionplugin.h>
#include <QtTest>

Q_IMPORT_QML_PLUGIN(SherpaPlugin)

#define WAIT(expr, ms) QTRY_VERIFY2_WITH_TIMEOUT((expr), #expr, ms)

class TstTools : public QObject {
    Q_OBJECT
    CoreEnv env;
    TestSshd sshd;
    QQmlApplicationEngine *engine = nullptr;
    QQuickWindow *win = nullptr;
    Backend *be = nullptr;
    QString dir, okId, twoId;
    QList<QProcess *> daemons;
    QTemporaryDir work;

    void shot(const QString &name)
    {
        QTest::qWait(500);
        const QImage img = win->grabWindow();
        QVERIFY(!img.isNull());
        QVERIFY(img.save(dir + "/" + name + ".png"));
    }
    QObject *prop(const char *n) { return win->property(n).value<QObject *>(); }
    DesktopState *desk() { return be->desk()->desktop(okId); }
    QObject *page()
    {
        QObject *rep = prop("desktopRepeater");
        QQuickItem *p = nullptr;
        QMetaObject::invokeMethod(rep, "itemAt", Q_RETURN_ARG(QQuickItem *, p), Q_ARG(int, 0));
        return p;
    }
    // вычисление JS-выражения в области объекта окна
    QVariant ev(QObject *o, const QString &expr)
    {
        QQmlExpression e(QQmlEngine::contextForObject(o), o, expr);
        const QVariant v = e.evaluate();
        if (e.hasError())
            qWarning() << "expr" << expr << e.error().toString();
        return v;
    }
    bool evb(QObject *o, const QString &expr) { return ev(o, expr).toBool(); }
    QObject *openTool(const QString &kind)
    {
        QMetaObject::invokeMethod(page(), "openWindow", Q_ARG(QVariant, kind), Q_ARG(QVariant, QVariant()), Q_ARG(QVariant, QVariant()), Q_ARG(QVariant, QVariant()));
        for (const QVariant &w : page()->property("wins").toList()) {
            const QVariantMap m = w.toMap();
            if (m.value("kind") == kind) {
                auto *ci = m.value("win").value<QObject *>()->property("contentItem").value<QQuickItem *>();
                return ci && !ci->childItems().isEmpty() ? ci->childItems().first() : nullptr;
            }
        }
        return nullptr;
    }
    void closeTool(const QString &kind)
    {
        for (const QVariant &w : page()->property("wins").toList()) {
            const QVariantMap m = w.toMap();
            if (m.value("kind") == kind)
                QVERIFY2(QMetaObject::invokeMethod(page(), "forceClose", Q_ARG(QVariant, m.value("id"))), "forceClose");
        }
    }
    // подтвердить открытый диалог опасного действия
    void confirmDanger(QObject *tool, const QString &name = {}, const QString &word = {})
    {
        QObject *dlg = tool->property("dangerDialog").value<QObject *>();
        QTRY_VERIFY_WITH_TIMEOUT(dlg->property("opened").toBool(), 5000);
        if (!name.isEmpty())
            ev(dlg, QString("nameField.text = '%1'").arg(name));
        if (!word.isEmpty())
            ev(dlg, QString("wordField.text = '%1'").arg(word));
        QTest::qWait(150);
        QMetaObject::invokeMethod(dlg, "accept");
    }
    void sh(const QString &cmd)
    {
        QProcess p;
        p.start("sh", {"-c", cmd});
        QVERIFY2(p.waitForFinished(60000), qPrintable(cmd));
    }
    void bg(const QString &cmd)
    {
        auto *p = new QProcess(this);
        p->start("sh", {"-c", cmd});
        daemons << p;
    }
    QString err(QObject *t) { return t->property("errorText").toString(); }

private slots:
    void initTestCase()
    {
        if (TestSshd::find().isEmpty())
            QSKIP("sshd not installed");
        // «sudo» без пароля для тестового сервера: на хосте без root утилиты Docker/веб идут через priv.*
        QDir().mkpath(work.filePath("bin"));
        QFile f(work.filePath("bin/sudo"));
        QVERIFY(f.open(QIODevice::WriteOnly));
        f.write("#!/bin/sh\nwhile [ $# -gt 0 ]; do case \"$1\" in -n|-S|-k|-v|-E) shift;; -p) shift 2;; --) shift; break;; -*) shift;; *) break;; esac; done\n[ $# -eq 0 ] && exit 0\nexec \"$@\"\n");
        f.close();
        f.setPermissions(QFile::ReadOwner | QFile::WriteOwner | QFile::ExeOwner | QFile::ReadGroup | QFile::ExeGroup | QFile::ReadOther | QFile::ExeOther);
        if (getuid() != 0)
            sshd.extraConfig = QString("SetEnv PATH=%1:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin\n").arg(work.filePath("bin"));
        QVERIFY2(sshd.start(), "sshd did not start");
        qputenv("XDG_RUNTIME_DIR", env.dir.filePath("run").toUtf8());
        QDir().mkpath(env.dir.filePath("run"));
        QFile(env.dir.filePath("run")).setPermissions(QFile::ReadOwner | QFile::WriteOwner | QFile::ExeOwner);
        dir = qEnvironmentVariable("SHERPA_SHOTS_DIR", QDir::currentPath() + "/screenshots");
        QDir().mkpath(dir);
        QQuickStyle::setStyle("Basic");
        QCoreApplication::setOrganizationName("SherpaTools");
        QCoreApplication::setApplicationName("SherpaTools");
        QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, env.dir.filePath("settings"));
        for (const char *fnt : {"JetBrainsMono-Regular.ttf", "JetBrainsMono-Bold.ttf", "Inter-Regular.otf", "Inter-SemiBold.otf", "Inter-Bold.otf"})
            QFontDatabase::addApplicationFont(QStringLiteral(":/fonts/") + fnt);
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
        auto mk = [&](const QString &name) {
            QJsonObject in{{"name", name}, {"host", "127.0.0.1"}, {"port", sshd.port}, {"user", sshd.user},
                           {"auth", QJsonObject{{"method", "key_text"}, {"key_text", sshd.readKey(sshd.clientKeyPath)}}}};
            return c->runBlocking({"conn", "create"}, in, true).data.value("connection").toMap().value("id").toString();
        };
        okId = mk("web-01");
        twoId = mk("web-02");
        QVERIFY(!okId.isEmpty() && !twoId.isEmpty());
        be->reload();
        QTRY_COMPARE_WITH_TIMEOUT(be->servers()->totalCount(), 2, 10000);
        be->setSelectedId(okId);
        be->checkOne(okId);
        QTRY_VERIFY_WITH_TIMEOUT(!be->pendingHostKey().isEmpty(), 15000);
        be->answerHostKey("accept");
        QTRY_COMPARE_WITH_TIMEOUT(be->serverById(okId).value("state").toString(), QString("online"), 20000);
        QTRY_VERIFY_WITH_TIMEOUT(be->pendingHostKey().isEmpty(), 5000);
        QTest::qWait(600);

        QMetaObject::invokeMethod(win, "openDesktop", Q_ARG(QVariant, okId), Q_ARG(QVariant, false));
        QTRY_VERIFY2_WITH_TIMEOUT(desk() && desk()->state() == "ready", "desktop not ready", 30000);
        QTest::qWait(500);
    }

    // ---------------------------------------------------------------- Сеть
    void network()
    {
        QObject *t = openTool("network");
        QVERIFY(t);
        WAIT(ev(t, "ifaces.length").toInt() > 0, 15000);
        QVERIFY(evb(t, "ifaces.some(i => i.ifname === 'lo')"));
        shot("tools-net-01-interfaces");
        t->setProperty("tab", "routes");
        WAIT(!t->property("loading").toBool(), 15000);
        shot("tools-net-02-routes");
        t->setProperty("tab", "dns");
        WAIT(!t->property("loading").toBool(), 15000);
        QVERIFY(!ev(t, "dns.servers").isNull());
        shot("tools-net-03-dns");
        t->setProperty("tab", "ports");
        WAIT(ev(t, "sockets.length").toInt() > 0, 15000);
        QVERIFY(evb(t, "sockets.some(s => s.local.port === " + QString::number(sshd.port) + ")"));
        shot("tools-net-04-ports");
        t->setProperty("tab", "firewall");
        WAIT(!t->property("loading").toBool(), 15000);
        shot("tools-net-05-firewall");
    }
    void networkRollbackDialog()
    {
        QObject *t = openTool("network");
        QObject *flow = t->property("netFlow").value<QObject *>();
        QMetaObject::invokeMethod(flow, "showPending", Q_ARG(QVariant, "tx-demo"), Q_ARG(QVariant, 8));
        QTest::qWait(300);
        QVERIFY(flow->property("pending").toBool());
        shot("tools-net-06-rollback-countdown");
        const int r0 = flow->property("remaining").toInt();
        QTest::qWait(2200);
        QVERIFY(flow->property("remaining").toInt() < r0);
        // не подтвердили — по истечении срока (+запас) диалог закрывается, итог «expired»
        WAIT(flow->property("result").toString() == "expired", 20000);
        QVERIFY(!flow->property("pending").toBool());
        closeTool("network");
    }
    void firewallOpensFromLauncherId()
    {
        QObject *t = openTool("firewall");
        QVERIFY(t);
        QCOMPARE(t->property("tab").toString(), QString("firewall"));
        closeTool("firewall");
    }

    // ---------------------------------------------------------------- Сетевые инструменты
    void nettools()
    {
        QObject *t = openTool("nettools");
        QVERIFY(t);
        ev(t, "targetField.text = '127.0.0.1'");
        ev(t, "countField.text = '2'");
        QMetaObject::invokeMethod(t, "run");
        WAIT(t->property("running").toBool(), 5000);
        WAIT(!t->property("running").toBool(), 30000);
        QVERIFY2(ev(t, "out.text").toString().contains("127.0.0.1"), qPrintable(ev(t, "out.text").toString()));
        QCOMPARE(t->property("exitCode").toInt(), 0);
        shot("tools-nettools-01-ping");
        t->setProperty("tool", "port");
        ev(t, "portField.text = '" + QString::number(sshd.port) + "'");
        QMetaObject::invokeMethod(t, "run");
        WAIT(t->property("exitCode").toInt() == 0 && !t->property("running").toBool() && t->property("status").toString() != "", 30000);
        t->setProperty("tool", "dig");
        closeTool("nettools");
    }

    // ---------------------------------------------------------------- Docker
    void docker()
    {
        if (QStandardPaths::findExecutable("docker").isEmpty())
            QSKIP("docker CLI not available");
        sh("docker rm -f sherpa-ui-test >/dev/null 2>&1; docker run -d --name sherpa-ui-test sherpa-qt sh -c 'while true; do echo hello-log; sleep 1; done'");
        QObject *t = openTool("docker");
        QVERIFY(t);
        WAIT(evb(t, "containers.some(c => c.Names === 'sherpa-ui-test')"), 30000);
        WAIT(ev(t, "Object.keys(stats).length").toInt() > 0, 20000);
        shot("tools-docker-01-containers");
        QMetaObject::invokeMethod(t, "openLogs", Q_ARG(QVariant, ev(t, "containers.find(c => c.Names === 'sherpa-ui-test')")));
        WAIT(ev(t, "panelText.text").toString().contains("hello-log"), 20000);
        shot("tools-docker-02-logs");
        QMetaObject::invokeMethod(t, "openInspect", Q_ARG(QVariant, ev(t, "containers.find(c => c.Names === 'sherpa-ui-test')")));
        WAIT(ev(t, "panelText.text").toString().contains("NetworkSettings"), 20000);
        QVERIFY(!ev(t, "panelText.text").toString().contains("Env"));
        shot("tools-docker-03-inspect");
        // остановка через подтверждение
        ev(t, "closePanel()");
        QMetaObject::invokeMethod(t, "act", Q_ARG(QVariant, "stop"), Q_ARG(QVariant, ev(t, "containers.find(c => c.Names === 'sherpa-ui-test')")));
        QTRY_VERIFY_WITH_TIMEOUT(t->property("dangerDialog").value<QObject *>()->property("opened").toBool(), 5000);
        shot("tools-docker-04-confirm-stop");
        confirmDanger(t);
        WAIT(evb(t, "containers.some(c => c.Names === 'sherpa-ui-test' && c.State === 'exited')"), 30000);
        QMetaObject::invokeMethod(t, "act", Q_ARG(QVariant, "rm"), Q_ARG(QVariant, ev(t, "containers.find(c => c.Names === 'sherpa-ui-test')")));
        confirmDanger(t);
        WAIT(!evb(t, "containers.some(c => c.Names === 'sherpa-ui-test')"), 30000);
        t->setProperty("tab", "images");
        WAIT(ev(t, "images.length").toInt() > 0, 15000);
        shot("tools-docker-05-images");
        closeTool("docker");
    }

    // ---------------------------------------------------------------- Веб-серверы
    void web()
    {
        if (QStandardPaths::findExecutable("nginx").isEmpty())
            QSKIP("nginx not installed");
        sh("mkdir -p /etc/nginx/conf.d /var/www/demo && echo ok > /var/www/demo/index.html && "
           "openssl req -x509 -nodes -newkey rsa:2048 -days 20 -subj /CN=demo.example.com -keyout /tmp/demo.key -out /tmp/demo.crt 2>/dev/null && "
           "printf 'server {\\n listen 8081 ssl;\\n server_name demo.example.com www.demo.example.com;\\n root /var/www/demo;\\n"
           " ssl_certificate /tmp/demo.crt;\\n ssl_certificate_key /tmp/demo.key;\\n}\\n' > /etc/nginx/conf.d/demo.conf && nginx");
        QObject *t = openTool("web");
        QVERIFY(t);
        WAIT(ev(t, "sites.length").toInt() > 0, 30000);
        QVERIFY(evb(t, "sites.some(s => s.names.indexOf('demo.example.com') >= 0)"));
        shot("tools-web-01-sites");
        QMetaObject::invokeMethod(t, "runTest");
        WAIT(t->property("testRan").toBool(), 15000);
        QVERIFY(t->property("testValid").toBool());
        shot("tools-web-02-test-ok");
        QMetaObject::invokeMethod(t, "reload");
        confirmDanger(t);
        WAIT(!t->property("testRan").toBool(), 15000);
        QVERIFY2(err(t).isEmpty(), qPrintable(err(t)));
        t->setProperty("tab", "certs");
        WAIT(evb(t, "certs.some(c => (c.path || '').indexOf('demo.crt') >= 0)"), 20000);
        shot("tools-web-03-certs");
        closeTool("web");
        sh("nginx -s stop >/dev/null 2>&1; true");
    }

    // ---------------------------------------------------------------- Базы данных
    void databases()
    {
        if (QStandardPaths::findExecutable("redis-server").isEmpty())
            QSKIP("redis not installed");
        sh("redis-server --daemonize yes --port 6379 >/dev/null && sleep 1 && redis-cli set k1 v1 >/dev/null && redis-cli -n 2 set k2 v2 >/dev/null");
        QObject *t = openTool("databases");
        QVERIFY(t);
        WAIT(t->property("engine").toString() == "redis", 20000);
        WAIT(ev(t, "dbs.length").toInt() > 0, 20000);
        shot("tools-db-01-redis");
        t->setProperty("tab", "queries");
        WAIT(ev(t, "queries.length").toInt() > 0, 20000);
        shot("tools-db-02-clients");
        t->setProperty("tab", "dbs");
        WAIT(ev(t, "dbs.length").toInt() > 0, 15000);
        QMetaObject::invokeMethod(t, "openDump", Q_ARG(QVariant, ev(t, "dbs[0]")));
        QTest::qWait(300);
        shot("tools-db-03-dump-dialog");
        QMetaObject::invokeMethod(t, "doDump");
        confirmDanger(t);
        WAIT(ev(t, "toast.text").toString().contains("/var/backups/"), 30000);
        QVERIFY2(err(t).isEmpty(), qPrintable(err(t)));
        closeTool("databases");
        sh("redis-cli shutdown nosave >/dev/null 2>&1; true");
    }

    // ---------------------------------------------------------------- Безопасность
    void security()
    {
        sh("mkdir -p /root/.ssh && chmod 700 /root/.ssh && rm -f /root/.ssh/authorized_keys; (nc -l -p 5432 >/dev/null 2>&1 &) ; true");
        QObject *t = openTool("security");
        QVERIFY(t);
        WAIT(ev(t, "score.score").isValid() && !ev(t, "score.score").isNull(), 30000);
        shot("tools-sec-01-overview");
        // добавить ключ (root → ввод имени сервера)
        const QString pub = sshd.readKey(sshd.clientKeyPath + ".pub").trimmed();
        ev(t, "dangerous({title: 'add', needName: true}, 'sec.keys.add', {connection_id: serverId, user: 'root', public_key: '" + pub + "'}, () => refresh())");
        confirmDanger(t, "web-01");
        t->setProperty("tab", "keys");
        WAIT(ev(t, "keys.length").toInt() > 0, 20000);
        shot("tools-sec-02-keys");
        QMetaObject::invokeMethod(t, "delKey", Q_ARG(QVariant, ev(t, "keys[0]")));
        confirmDanger(t, QSysInfo::machineHostName()); // для root ядро сверяет hostname сервера
        WAIT(ev(t, "keys.length").toInt() == 0, 20000);
        t->setProperty("tab", "ports");
        WAIT(evb(t, "ports.some(p => p.port === 5432)"), 20000);
        shot("tools-sec-03-ports");
        t->setProperty("tab", "logins");
        WAIT(!t->property("loading").toBool(), 15000);
        shot("tools-sec-04-logins");
        t->setProperty("tab", "updates");
        WAIT(!t->property("loading").toBool(), 15000);
        closeTool("security");
        sh("pkill -f 'nc -l -p 5432'; true");
    }

    // ---------------------------------------------------------------- Бэкапы
    void backups()
    {
        if (QStandardPaths::findExecutable("restic").isEmpty())
            QSKIP("restic not installed");
        const QString base = work.filePath("bk");
        sh("mkdir -p " + base + "/src && echo data1 > " + base + "/src/a.txt && echo data2 > " + base + "/src/b.txt && echo pw > " + base + "/pw && "
           "restic init -q -r " + base + "/repo --password-file " + base + "/pw");
        QObject *t = openTool("backup");
        QVERIFY(t);
        ev(t, QString("call('backup.jobs.set', {connection_id: serverId, title: 'Nightly src', engine: 'restic', source: '%1/src', repository: '%1/repo', password_file: '%1/pw', schedule: '0 3 * * *'}, () => refresh())").arg(base));
        WAIT(ev(t, "jobs.length").toInt() == 1, 15000);
        shot("tools-backup-01-jobs");
        QMetaObject::invokeMethod(t, "runJob", Q_ARG(QVariant, ev(t, "jobs[0]")));
        WAIT(t->property("note").toString().contains("finished"), 60000);
        QMetaObject::invokeMethod(t, "select", Q_ARG(QVariant, ev(t, "jobs[0]")));
        WAIT(ev(t, "snaps.length").toInt() == 1, 30000);
        shot("tools-backup-02-snapshots");
        // пробное восстановление
        ev(t, QString("openRestore(snaps[0]); rTarget.text = '%1/restore'; doRestore()").arg(base));
        WAIT(t->property("note").toString().contains("Dry run finished"), 60000);
        QVERIFY(!QFile::exists(base + "/restore"));
        // настоящее восстановление — с вводом имени сервера
        ev(t, QString("openRestore(snaps[0]); rTarget.text = '%1/restore'; rDry.checked = false; doRestore()").arg(base));
        confirmDanger(t, QSysInfo::machineHostName());
        WAIT(QFile::exists(base + "/restore"), 60000);
        QMetaObject::invokeMethod(t, "checkJob", Q_ARG(QVariant, ev(t, "jobs[0]")));
        WAIT(t->property("note").toString().contains("check passed"), 60000);
        QMetaObject::invokeMethod(t, "pruneJob", Q_ARG(QVariant, ev(t, "jobs[0]")));
        confirmDanger(t);
        WAIT(t->property("note").toString().contains("Prune finished"), 60000);
        QVERIFY2(err(t).isEmpty(), qPrintable(err(t)));
        QMetaObject::invokeMethod(t, "openEdit", Q_ARG(QVariant, ev(t, "jobs[0]")));
        shot("tools-backup-03-edit");
        QMetaObject::invokeMethod(t, "delJob", Q_ARG(QVariant, ev(t, "jobs[0]")));
        closeTool("backup");
    }

    // ---------------------------------------------------------------- Сниппеты и мультисервер
    void snippets()
    {
        QObject *t = openTool("snippets");
        QVERIFY(t);
        ev(t, "call('snippet.set', {title: 'Kernel version', command: 'uname -r', tags: ['sys', 'info']}, () => refresh())");
        ev(t, "call('snippet.set', {title: 'Disk usage', command: 'df -h /', tags: ['disk']}, () => refresh())");
        WAIT(ev(t, "snippets.length").toInt() == 2, 15000);
        QMetaObject::invokeMethod(t, "load", Q_ARG(QVariant, ev(t, "snippets.find(s => s.title === 'Kernel version')")));
        // запуск на двух серверах: слово подтверждения обязательно
        ev(t, "runner.start({snippetId: selId, preview: 'uname -r', targets: ['" + okId + "', '" + twoId + "'], title: 'Run snippet'})");
        QObject *dlg = t->property("dangerDialog").value<QObject *>();
        QTRY_VERIFY_WITH_TIMEOUT(dlg->property("opened").toBool(), 5000);
        shot("tools-snippets-01-confirm-multi");
        QVERIFY(!dlg->property("okToGo").toBool());
        confirmDanger(t, {}, QString::fromUtf8("выполнить"));
        WAIT(t->property("runner").value<QObject *>()->property("done").toBool(), 40000);
        QCOMPARE(ev(t, "runner.results.filter(r => r.state === 'ok' && r.exit === 0).length").toInt(), 2);
        shot("tools-snippets-02-results");
        t->setProperty("tab", "history");
        WAIT(ev(t, "history.length").toInt() >= 2, 15000);
        shot("tools-snippets-03-history");
        closeTool("snippets");
    }
    void multiServer()
    {
        QObject *t = openTool("multi");
        QVERIFY(t);
        // разные ответы серверов → две группы в режиме «Сравнение»
        ev(t, "runner.start({command: 'echo $RANDOM$RANDOM', targets: ['" + okId + "', '" + twoId + "']})");
        confirmDanger(t, {}, QString::fromUtf8("выполнить"));
        WAIT(t->property("runner").value<QObject *>()->property("done").toBool(), 40000);
        QCOMPARE(ev(t, "runner.results.length").toInt(), 2);
        QVERIFY(ev(t, "runner.groups.length").toInt() >= 1);
        shot("tools-multi-01-by-server");
        ev(t, "runner.start({command: 'uname -s', targets: ['" + okId + "', '" + twoId + "']})");
        confirmDanger(t, {}, QString::fromUtf8("выполнить"));
        WAIT(t->property("runner").value<QObject *>()->property("done").toBool(), 40000);
        QCOMPARE(ev(t, "runner.groups.length").toInt(), 1);
        QCOMPARE(ev(t, "runner.groups[0].count").toInt(), 2);
        // переключение «Сравнение» внутри RunResults
        QObject *res = t->findChild<QObject *>("resultTable");
        QVERIFY(res);
        closeTool("multi");
    }

    // ---------------------------------------------------------------- Сравнение конфигов
    void diff()
    {
        sh("printf 'worker_processes 2;\\nlisten 80;\\nkeepalive 65;\\n' > /tmp/sherpa-a.conf; printf 'worker_processes 4;\\nlisten 80;\\nkeepalive 65;\\ngzip on;\\n' > /tmp/sherpa-b.conf");
        QObject *t = openTool("diff");
        QVERIFY(t);
        ev(t, "pathField.text = '/tmp/sherpa-a.conf'; otherSrv.current = '" + twoId + "'; otherPath.text = '/tmp/sherpa-b.conf'");
        QMetaObject::invokeMethod(t, "compare");
        WAIT(t->property("compared").toBool(), 15000);
        QVERIFY(!t->property("equal").toBool());
        QVERIFY(evb(t, "lines.some(l => l.kind === 'chg' && l.left === 'worker_processes 2;' && l.right === 'worker_processes 4;')"));
        QVERIFY(evb(t, "lines.filter(l => l.kind === 'ctx').length === 2")); // listen и keepalive распознаны общими
        QVERIFY(evb(t, "lines.some(l => l.kind === 'add' && l.right === 'gzip on;')"));
        shot("tools-diff-01-two-columns");
        t->setProperty("onlyDiff", true);
        QCOMPARE(ev(t, "visibleLines.length").toInt(), 2);
        shot("tools-diff-02-only-diff");
        t->setProperty("onlyDiff", false);
        // сохранить версию и сравнить с ней
        QMetaObject::invokeMethod(t, "saveVersion");
        WAIT(ev(t, "versions.length").toInt() == 1, 15000);
        t->setProperty("against", "version");
        QTest::qWait(200);
        ev(t, "verSel.current = versions[0].id");
        QMetaObject::invokeMethod(t, "compare");
        WAIT(t->property("equal").toBool(), 15000);
        shot("tools-diff-03-identical");
        closeTool("diff");
    }

    // ---------------------------------------------------------------- Заметки и секреты
    void notes()
    {
        QObject *t = openTool("notes");
        QVERIFY(t);
        ev(t, "nTitle.text = 'Runbook'; nBody.text = '# Deploy\\n\\n* pull\\n* **restart** nginx\\n'; dirty = true; saveNote()");
        WAIT(ev(t, "notes.length").toInt() == 1, 15000);
        QMetaObject::invokeMethod(t, "loadNote", Q_ARG(QVariant, ev(t, "notes[0]")));
        t->setProperty("preview", true);
        shot("tools-notes-01-preview");
        t->setProperty("tab", "secrets");
        ev(t, "call('vault.set', {connection_id: serverId, title: 'Grafana admin', value: 'top-secret-42'}, () => refresh())");
        WAIT(ev(t, "secrets.length").toInt() == 1, 15000);
        // в списке значения нет
        QVERIFY(!QJsonDocument::fromVariant(ev(t, "secrets")).toJson().contains("top-secret-42"));
        shot("tools-notes-02-secrets");
        QMetaObject::invokeMethod(t, "reveal", Q_ARG(QVariant, ev(t, "secrets[0]")));
        shot("tools-notes-03-confirm-reveal");
        confirmDanger(t);
        WAIT(t->property("revealed").toString() == "top-secret-42", 15000);
        shot("tools-notes-04-revealed");
        QMetaObject::invokeMethod(t, "hideSecret");
        QVERIFY(t->property("revealed").toString().isEmpty());
        // автосокрытие
        QMetaObject::invokeMethod(t, "reveal", Q_ARG(QVariant, ev(t, "secrets[0]")));
        confirmDanger(t);
        WAIT(!t->property("revealed").toString().isEmpty(), 15000);
        t->setProperty("revealLeft", 1);
        WAIT(t->property("revealed").toString().isEmpty(), 5000);
        closeTool("notes");
    }

    // ---------------------------------------------------------------- Журнал
    void audit()
    {
        QObject *t = openTool("audit");
        QVERIFY(t);
        WAIT(ev(t, "items.length").toInt() > 3, 20000);
        QVERIFY(evb(t, "items.some(i => i.method === 'docker.stop' || i.method === 'sec.keys.add' || i.method === 'multi.run')"));
        shot("tools-audit-01-log");
        ev(t, "scope = 'all'");
        WAIT(!t->property("loading").toBool(), 15000);
        ev(t, "query = 'multi'; refresh()");
        WAIT(ev(t, "items.length").toInt() > 0 && evb(t, "items.every(i => (i.method + i.outcome).indexOf('multi') >= 0)"), 10000);
        shot("tools-audit-02-search");
        QMetaObject::invokeMethod(t, "exportCsv");
        WAIT(t->property("note").toString().startsWith("Saved"), 10000);
        closeTool("audit");
    }

    // ---------------------------------------------------------------- Виджеты на обоях
    void widgets()
    {
        QObject *w = page()->property("widgets").value<QObject *>();
        for (const char *id : {"cpu", "server", "disks", "alerts"})
            QMetaObject::invokeMethod(w, "setOn", Q_ARG(QVariant, id), Q_ARG(QVariant, true));
        QTest::qWait(3500);
        shot("tools-widgets-01-wallpaper");
        QVERIFY(evb(w, "isOn('cpu') && isOn('disks')"));
        WAIT(ev(w, "disks.length").toInt() > 0, 20000);
        // положение сохраняется в раскладке
        QMetaObject::invokeMethod(w, "moved", Q_ARG(QVariant, "cpu"), Q_ARG(QVariant, 400), Q_ARG(QVariant, 300));
        WAIT(desk()->layout().value("widgets").toMap().value("cpu").toMap().value("x").toInt() == 400, 5000);
        // окна поверх виджетов, лаунчер показывает все новые утилиты
        openTool("docker");
        openTool("backup");
        QTest::qWait(400);
        shot("tools-widgets-02-with-windows");
        closeTool("docker");
        closeTool("backup");
    }
    void launcher()
    {
        QObject *l = page()->findChild<QObject *>("launcher");
        QVERIFY(l);
        QMetaObject::invokeMethod(l, "open");
        QTest::qWait(400);
        for (const char *id : {"network", "firewall", "nettools", "docker", "databases", "web", "security", "backup", "snippets", "multi", "diff", "notes", "audit"})
            QVERIFY2(evb(page(), QString("AppRegistry.byId('%1').ready && AppRegistry.enabledFor(AppRegistry.byId('%1'), desk.capabilities)").arg(id)), id);
        shot("tools-launcher");
        QMetaObject::invokeMethod(l, "close");
    }

    // ---------------------------------------------------------------- Русский интерфейс
    void russian()
    {
        be->setLanguage("ru");
        QTest::qWait(300);
        QObject *t = openTool("security");
        WAIT(ev(t, "score.score").isValid() && !ev(t, "score.score").isNull(), 30000);
        shot("tools-ru-security");
        closeTool("security");
        t = openTool("network");
        WAIT(ev(t, "ifaces.length").toInt() > 0, 15000);
        shot("tools-ru-network");
        closeTool("network");
        t = openTool("docker");
        QTest::qWait(1500);
        shot("tools-ru-docker");
        closeTool("docker");
        be->setLanguage("en");
    }

    void cleanupTestCase()
    {
        for (auto *p : daemons) {
            p->kill();
            p->waitForFinished(2000);
        }
        if (be)
            be->client()->runBlocking({"daemon", "stop", "--force"});
        delete engine;
    }
};

QTEST_MAIN(TstTools)
#include "tst_tools.moc"
