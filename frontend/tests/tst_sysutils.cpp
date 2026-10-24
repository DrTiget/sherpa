// Окна системных утилит (диски, логи, планировщик, службы, пакеты, система) на живом демоне и sshd.
// По умолчанию поднимается локальный sshd (в контейнере — от root, без systemd). SHERPA_EXT_SSH="порт|пользователь|ключ"
// подключает уже запущенный sshd (например, хоста с systemd) — тогда окна показывают настоящие службы и журнал.
// Скриншоты — в SHERPA_SHOTS_DIR. Без sshd — пропускается.
#include "Backend.h"
#include "CoreEnv.h"
#include "TestSshd.h"

#include <QFontDatabase>
#include <QJsonArray>
#include <QQmlApplicationEngine>
#include <QQuickItem>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QtQml/qqmlextensionplugin.h>
#include <QtTest>

Q_IMPORT_QML_PLUGIN(SherpaPlugin)

class TstSysUtils : public QObject {
    Q_OBJECT
    CoreEnv env;
    TestSshd sshd;
    QQmlApplicationEngine *engine = nullptr;
    QQuickWindow *win = nullptr;
    Backend *be = nullptr;
    QString dir, connId, extUser, extKey;
    int extPort = 0;
    bool isRoot = false;
    bool canMutate() { return isRoot || desk()->capabilities().value("privileges").toMap().value("sudo_nopasswd").toBool(); }

    void shot(const QString &name)
    {
        QTest::qWait(500);
        const QImage img = win->grabWindow();
        QVERIFY(!img.isNull());
        QVERIFY(img.save(dir + "/" + name + ".png"));
    }
    DesktopState *desk() { return be->desk()->desktop(connId); }
    QObject *page()
    {
        QObject *rep = win->property("desktopRepeater").value<QObject *>();
        QQuickItem *p = nullptr;
        QMetaObject::invokeMethod(rep, "itemAt", Q_RETURN_ARG(QQuickItem *, p), Q_ARG(int, 0));
        return p;
    }
    // окно утилиты → корневой UtilPane
    QObject *open(const QString &kind, bool exclusive = true)
    {
        if (exclusive)
            closeAll();
        QMetaObject::invokeMethod(page(), "openWindow", Q_ARG(QVariant, kind), Q_ARG(QVariant, QVariant()), Q_ARG(QVariant, QVariant()), Q_ARG(QVariant, QVariant()));
        for (const QVariant &w : page()->property("wins").toList()) {
            const QVariantMap m = w.toMap();
            if (m.value("kind") == kind) {
                QObject *sw = m.value("win").value<QObject *>();
                auto *ci = sw->property("contentItem").value<QQuickItem *>();
                return ci->childItems().value(0);
            }
        }
        return nullptr;
    }
    void closeAll()
    {
        for (const QVariant &w : page()->property("wins").toList())
            QMetaObject::invokeMethod(page(), "forceClose", Q_ARG(QVariant, w.toMap().value("id")));
    }
    bool util(const char *name) { return desk()->capabilities().value("utilities").toMap().value(name).toBool(); }
    bool tool(const char *name) { return desk()->capabilities().value("tools").toMap().value(name).toBool(); }
    QObject *child(QObject *root, const char *name) { return root->findChild<QObject *>(name); }
    void click(QObject *root, const char *name)
    {
        QObject *o = child(root, name);
        QVERIFY2(o, name);
        QMetaObject::invokeMethod(o, "clicked");
    }
    void setText(QObject *root, const char *name, const QString &t)
    {
        QObject *o = child(root, name);
        QVERIFY2(o, name);
        o->setProperty("text", t);
    }
    // вызов функции QML-корня окна с произвольными аргументами
    void js(QObject *o, const char *m, const QVariantList &a)
    {
        bool ok = false;
        switch (a.size()) {
        case 1: ok = QMetaObject::invokeMethod(o, m, Q_ARG(QVariant, a[0])); break;
        case 2: ok = QMetaObject::invokeMethod(o, m, Q_ARG(QVariant, a[0]), Q_ARG(QVariant, a[1])); break;
        case 3: ok = QMetaObject::invokeMethod(o, m, Q_ARG(QVariant, a[0]), Q_ARG(QVariant, a[1]), Q_ARG(QVariant, a[2])); break;
        case 4: ok = QMetaObject::invokeMethod(o, m, Q_ARG(QVariant, a[0]), Q_ARG(QVariant, a[1]), Q_ARG(QVariant, a[2]), Q_ARG(QVariant, a[3])); break;
        case 5: ok = QMetaObject::invokeMethod(o, m, Q_ARG(QVariant, a[0]), Q_ARG(QVariant, a[1]), Q_ARG(QVariant, a[2]), Q_ARG(QVariant, a[3]), Q_ARG(QVariant, a[4])); break;
        default: ok = QMetaObject::invokeMethod(o, m);
        }
        QVERIFY2(ok, m);
    }
    QVariant prop(QObject *o, const char *n) { return o->property(n); }
    int len(QObject *o, const char *n) { return o->property(n).toList().size(); }

private slots:
    void initTestCase()
    {
        const QString ext = qEnvironmentVariable("SHERPA_EXT_SSH");
        if (ext.isEmpty()) {
            if (TestSshd::find().isEmpty())
                QSKIP("sshd not installed");
            QVERIFY2(sshd.start(), "sshd did not start");
            extPort = sshd.port; extUser = sshd.user; extKey = sshd.clientKeyPath;
        } else {
            const QStringList f = ext.split('|');
            extPort = f[0].toInt(); extUser = f[1]; extKey = f[2];
        }
        qputenv("XDG_RUNTIME_DIR", env.dir.filePath("run").toUtf8());
        QDir().mkpath(env.dir.filePath("run"));
        QFile(env.dir.filePath("run")).setPermissions(QFile::ReadOwner | QFile::WriteOwner | QFile::ExeOwner);
        dir = qEnvironmentVariable("SHERPA_SHOTS_DIR", QDir::currentPath() + "/screenshots");
        QDir().mkpath(dir);
        QQuickStyle::setStyle("Basic");
        QCoreApplication::setOrganizationName("SherpaSysUtils");
        QCoreApplication::setApplicationName("SherpaSysUtils");
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
        QFile kf(extKey);
        QVERIFY(kf.open(QIODevice::ReadOnly));
        QJsonObject in{{"name", "sys-live"}, {"host", "127.0.0.1"}, {"port", extPort}, {"user", extUser},
                       {"auth", QJsonObject{{"method", "key_text"}, {"key_text", QString::fromUtf8(kf.readAll())}}}};
        auto r = be->client()->runBlocking({"conn", "create"}, in, true);
        connId = r.data.value("connection").toMap().value("id").toString();
        QVERIFY(!connId.isEmpty());
        be->reload();
        QTRY_COMPARE_WITH_TIMEOUT(be->servers()->totalCount(), 1, 10000);
        be->setSelectedId(connId);
        be->checkOne(connId);
        QTRY_VERIFY_WITH_TIMEOUT(!be->pendingHostKey().isEmpty(), 15000);
        QTest::qWait(600);   // диалог должен успеть открыться, иначе закрытие по ответу его не заденет
        be->answerHostKey("accept");
        QTRY_COMPARE_WITH_TIMEOUT(be->serverById(connId).value("state").toString(), QString("online"), 20000);
        QTRY_VERIFY_WITH_TIMEOUT(be->pendingHostKey().isEmpty(), 5000);
        QMetaObject::invokeMethod(win, "openDesktop", Q_ARG(QVariant, connId), Q_ARG(QVariant, false));
        QTRY_VERIFY_WITH_TIMEOUT(desk() && desk()->state() == "ready", 30000);
        isRoot = desk()->capabilities().value("user").toMap().value("is_root").toBool();
        QVERIFY(page());
        QTest::qWait(500);
    }

    void launcherListsUtilities()
    {
        QObject *l = page()->findChild<QObject *>("launcher");
        QVERIFY(l);
        const QVariantList apps = l->property("apps").toList();
        for (const char *id : {"services", "logs", "disks", "packages", "cron", "system"}) {
            bool found = false;
            for (const QVariant &a : apps)
                if (a.toMap().value("id") == id) { found = true; QVERIFY(a.toMap().value("ready").toBool()); }
            QVERIFY2(found, id);
        }
        QMetaObject::invokeMethod(l, "open");
        shot("sys-00-launcher");
        QMetaObject::invokeMethod(l, "close");
    }

    // ---------------------------------------------------------------- Система
    void systemWindow()
    {
        QObject *p = open("system");
        QVERIFY(p);
        QTRY_VERIFY_WITH_TIMEOUT(!prop(p, "info").toMap().value("hostname").toString().isEmpty(), 20000);
        QTRY_VERIFY_WITH_TIMEOUT(len(p, "users") > 0, 20000);
        QTRY_VERIFY_WITH_TIMEOUT(len(p, "groups") > 0, 20000);
        QCOMPARE(prop(p, "info").toMap().value("hostname").toString(), prop(p, "hostname").toString());
        shot("sys-10-system-general");
        p->setProperty("tab", "time");
        QTRY_VERIFY_WITH_TIMEOUT(!prop(p, "timeInfo").toMap().value("timezone").toString().isEmpty() || !prop(p, "timeError").toString().isEmpty(), 15000);
        if (!qEnvironmentVariableIsEmpty("SHERPA_EXT_SSH"))
            QVERIFY2(!prop(p, "timeInfo").toMap().value("timezone").toString().isEmpty(), qPrintable(prop(p, "timeError").toString()));
        shot("sys-11-system-time");
        p->setProperty("tab", "users");
        QTest::qWait(300);
        shot("sys-12-system-users");
        // пользователь из списка совпадает с getent
        bool found = false;
        for (const QVariant &u : p->property("users").toList())
            if (u.toMap().value("name").toString() == extUser) found = true;
        QVERIFY(found);
        p->setProperty("tab", "groups");
        QTest::qWait(300);
        shot("sys-13-system-groups");
    }
    void systemGroupLifecycle()
    {
        if (!isRoot || !tool("groupadd"))
            QSKIP("нужны root и groupadd");
        QObject *p = open("system");
        p->setProperty("tab", "groups");
        QMetaObject::invokeMethod(child(p, "groupAdd"), "clicked");
        QTest::qWait(300);
        setText(p, "newGroupName", "sherpatst");
        click(p, "newGroupOk");
        auto has = [&]() { for (const QVariant &g : p->property("groups").toList()) if (g.toMap().value("name") == "sherpatst") return true; return false; };
        QTRY_VERIFY_WITH_TIMEOUT(has(), 20000);
        // удаление — через опасный диалог с вводом имени сервера
        js(p, "danger", QVariantList{"sys.group.remove", QVariantMap{{"name", "sherpatst"}},
           QVariantMap{{"title", "Delete group"}, {"message", "test"}, {"confirmText", "Delete"}, {"needName", true}}, QVariant()});
        QTRY_VERIFY_WITH_TIMEOUT(!prop(p, "hostname").toString().isEmpty(), 10000);
        QObject *btn = child(p, "dangerConfirm");
        QVERIFY(btn);
        QVERIFY(!btn->property("enabled").toBool());          // без имени сервера кнопка неактивна
        setText(p, "dangerName", "wrong-name");
        QVERIFY(!btn->property("enabled").toBool());
        shot("sys-14-danger-dialog");
        setText(p, "dangerName", prop(p, "hostname").toString());
        QVERIFY(btn->property("enabled").toBool());
        QMetaObject::invokeMethod(btn, "clicked");
        QTRY_VERIFY_WITH_TIMEOUT(!has() || (QMetaObject::invokeMethod(p, "loadUsers"), false), 20000);
    }
    void systemUserAdd()
    {
        if (!isRoot || !tool("useradd"))
            QSKIP("нужны root и useradd");
        QObject *p = open("system");
        p->setProperty("tab", "users");
        QMetaObject::invokeMethod(child(p, "userAdd"), "clicked");
        QTest::qWait(300);
        setText(p, "newUserName", "sherpatstu");
        click(p, "newUserOk");
        auto has = [&]() { for (const QVariant &u : p->property("users").toList()) if (u.toMap().value("name") == "sherpatstu") return true; return false; };
        QTRY_VERIFY_WITH_TIMEOUT(has(), 20000);
        js(p, "runDanger", QVariantList{"sys.user.remove", QVariantMap{{"name", "sherpatstu"}, {"remove_home", true}}, prop(p, "hostname"), QVariant(), QVariant()});
        QTRY_VERIFY_WITH_TIMEOUT((QMetaObject::invokeMethod(p, "loadUsers"), !has()), 30000);
    }

    // ---------------------------------------------------------------- Службы
    void servicesWindow()
    {
        QObject *p = open("services");
        QVERIFY(p);
        if (!util("services")) {
            QTRY_VERIFY_WITH_TIMEOUT(p->property("unavailable").toBool(), 5000);
            shot("sys-20-services-unavailable");
            return;
        }
        QTRY_VERIFY2_WITH_TIMEOUT(len(p, "all") > 0, qPrintable("services: " + prop(p, "notice").toString()), 30000);
        QVERIFY(len(p, "rows") > 0);
        shot("sys-20-services");
        p->setProperty("filterMode", "running");
        QTRY_VERIFY_WITH_TIMEOUT(len(p, "rows") > 0, 5000);
        for (const QVariant &s : p->property("rows").toList())
            QCOMPARE(s.toMap().value("sub_state").toString(), QString("running"));
        p->setProperty("filterMode", "all");
        const QVariantMap first = p->property("rows").toList().first().toMap();
        QMetaObject::invokeMethod(p, "select", Q_ARG(QVariant, first));
        QTRY_VERIFY_WITH_TIMEOUT(len(p, "journal") >= 0 && p->property("sel").isValid(), 5000);
        QTest::qWait(1500);
        shot("sys-21-services-card");
        // поиск
        p->setProperty("query", "ssh");
        for (const QVariant &s : p->property("rows").toList())
            QVERIFY(s.toMap().value("name").toString().contains("ssh") || s.toMap().value("description").toString().toLower().contains("ssh"));
        p->setProperty("query", "");
    }

    // ---------------------------------------------------------------- Планировщик
    void cronWindow()
    {
        QObject *p = open("cron");
        QVERIFY(p);
        if (!util("cron")) {
            QTRY_VERIFY_WITH_TIMEOUT(p->property("unavailable").toBool(), 5000);
            shot("sys-30-cron-unavailable");
            return;
        }
        QTRY_VERIFY_WITH_TIMEOUT(!prop(p, "loading").toBool(), 20000);
        const QVariantList before = p->property("jobs").toList();
        QMetaObject::invokeMethod(child(p, "cronAdd"), "clicked");
        QTest::qWait(300);
        setText(p, "cronExpr", "*/7 3 * * *");
        setText(p, "cronCmd", "/bin/true # sherpa-ui-test");
        shot("sys-31-cron-editor");
        click(p, "cronSave");
        if (!canMutate()) {
            // без root и NOPASSWD ядро отвечает priv.required — окно просит пароль sudo и не теряет введённое
            QObject *sd = child(p, "sudoDialog");
            QVERIFY(sd);
            QTRY_VERIFY_WITH_TIMEOUT(sd->property("opened").toBool(), 20000);
            shot("sys-31b-cron-sudo");
            QMetaObject::invokeMethod(sd, "close");
            QTRY_VERIFY_WITH_TIMEOUT(!sd->property("opened").toBool(), 5000);
            QMetaObject::invokeMethod(child(p, "cronEditor"), "close");
            p->setProperty("tab", "timers");
            if (tool("systemctl")) {
                QTRY_VERIFY_WITH_TIMEOUT(len(p, "timers") > 0, 20000);
                shot("sys-33-cron-timers");
            }
            p->setProperty("tab", "system");
            QTest::qWait(1500);
            shot("sys-34-cron-system");
            return;
        }
        auto mine = [&]() { for (const QVariant &j : p->property("jobs").toList()) if (j.toMap().value("command").toString().contains("sherpa-ui-test")) return j.toMap(); return QVariantMap(); };
        QTRY_VERIFY_WITH_TIMEOUT(!mine().isEmpty(), 20000);
        QCOMPARE(p->property("jobs").toList().size(), before.size() + 1);
        QCOMPARE(mine().value("schedule").toString(), QString("*/7 3 * * *"));
        shot("sys-32-cron-list");
        // отключить → включить
        js(p, "toggle", QVariantList{mine()});
        QTRY_VERIFY_WITH_TIMEOUT(!mine().value("enabled").toBool(), 20000);
        js(p, "toggle", QVariantList{mine()});
        QTRY_VERIFY_WITH_TIMEOUT(mine().value("enabled").toBool(), 20000);
        // удалить (подтверждение confirm:true)
        js(p, "mutate", QVariantList{"cron.del", QVariantMap{{"_row", mine()}, {"line", mine().value("line")}, {"confirm", true}}, "x"});
        QTRY_VERIFY_WITH_TIMEOUT(mine().isEmpty(), 20000);
        p->setProperty("tab", "timers");
        if (tool("systemctl")) {
            QTRY_VERIFY_WITH_TIMEOUT(len(p, "timers") > 0, 20000);
            shot("sys-33-cron-timers");
        }
        p->setProperty("tab", "system");
        QTest::qWait(1500);
        shot("sys-34-cron-system");
    }

    // ---------------------------------------------------------------- Логи
    void logsWindow()
    {
        QObject *p = open("logs");
        QVERIFY(p);
        if (!util("logs")) {
            QTRY_VERIFY_WITH_TIMEOUT(p->property("unavailable").toBool(), 5000);
            shot("sys-40-logs-unavailable");
            return;
        }
        QTRY_VERIFY_WITH_TIMEOUT(!prop(p, "loading").toBool(), 30000);
        QTRY_VERIFY_WITH_TIMEOUT(prop(p, "sources").toMap().contains("files"), 10000);
        shot("sys-40-logs");
        const bool realJournal = !qEnvironmentVariableIsEmpty("SHERPA_EXT_SSH");   // logger пишет в журнал хоста, а не контейнера
        if (prop(p, "sources").toMap().value("journal").toBool() && !realJournal) {
            click(p, "logFollow");
            QTRY_VERIFY_WITH_TIMEOUT(prop(p, "following").toBool(), 15000);
            click(p, "logFollow");
            QTRY_VERIFY_WITH_TIMEOUT(!prop(p, "following").toBool(), 5000);
        }
        if (prop(p, "sources").toMap().value("journal").toBool() && realJournal) {
            // follow: новая запись из logger появляется в списке
            click(p, "logFollow");
            QTRY_VERIFY_WITH_TIMEOUT(prop(p, "following").toBool(), 15000);
            const QString marker = "sherpa-ui-marker-" + QString::number(QRandomGenerator::global()->generate());
            // запись в журнал хоста: через ssh на внешний sshd
            auto sshRun = [&](const QString &cmd) {
                QProcess pr;
                pr.start("ssh", {"-o", "StrictHostKeyChecking=no", "-o", "UserKnownHostsFile=/dev/null", "-o", "LogLevel=ERROR", "-i", extKey, "-p", QString::number(extPort), extUser + "@127.0.0.1", cmd});
                pr.waitForFinished(20000);
                return QString::fromUtf8(pr.readAllStandardOutput());
            };
            sshRun("systemd-cat -t sherpa-test echo " + marker);
            // непривилегированный пользователь не всегда видит свои записи в журнале хоста — тогда проверяем только запуск/остановку follow
            const bool visible = sshRun("sleep 1; journalctl --no-pager -n 50 2>/dev/null | grep -c " + marker).trimmed() != "0";
            if (!visible)
                QWARN("запись из systemd-cat не видна пользователю sshd: проверяется только запуск follow");
            auto seen = [&]() { for (const QVariant &e : p->property("entries").toList()) if (e.toMap().value("msg").toString().contains(marker)) return true; return false; };
            if (visible)
                QTRY_VERIFY_WITH_TIMEOUT(seen(), 20000);
            shot("sys-41-logs-follow");
            click(p, "logFollow");
            QTRY_VERIFY_WITH_TIMEOUT(!prop(p, "following").toBool(), 5000);
            // фильтр grep
            p->setProperty("grep", visible ? marker : "sherpa-no-such-line-" + marker);
            QMetaObject::invokeMethod(p, "reload");
            QTRY_VERIFY_WITH_TIMEOUT(!prop(p, "loading").toBool(), 15000);
            if (visible) {
                QTRY_VERIFY_WITH_TIMEOUT(seen(), 5000);
                QVERIFY(len(p, "entries") <= 3);
            } else {
                QCOMPARE(len(p, "entries"), 0);   // фильтр отсекает всё
            }
            p->setProperty("grep", "");
        }
        const QStringList files = prop(p, "sources").toMap().value("files").toStringList();
        if (!files.isEmpty()) {
            p->setProperty("source", files.first());
            QMetaObject::invokeMethod(p, "reload");
            QTRY_VERIFY_WITH_TIMEOUT(!prop(p, "loading").toBool(), 15000);
            shot("sys-42-logs-file");
        }
    }

    // ---------------------------------------------------------------- Пакеты
    void packagesWindow()
    {
        QObject *p = open("packages");
        QVERIFY(p);
        if (!util("packages")) {
            QTRY_VERIFY_WITH_TIMEOUT(p->property("unavailable").toBool(), 5000);
            shot("sys-50-packages-unavailable");
            return;
        }
        QTRY_VERIFY_WITH_TIMEOUT(!prop(p, "manager").toString().isEmpty(), 20000);
        QTRY_VERIFY_WITH_TIMEOUT(len(p, "installed") > 10, 40000);
        p->setProperty("tab", "installed");
        shot("sys-50-packages-installed");
        p->setProperty("query", "base");
        QVERIFY(len(p, "shownInstalled") <= len(p, "installed"));
        p->setProperty("query", "");
        p->setProperty("tab", "updates");
        shot("sys-51-packages-updates");
        // поиск
        p->setProperty("tab", "search");
        p->setProperty("searchText", "bash");
        QMetaObject::invokeMethod(p, "search");
        QTRY_VERIFY_WITH_TIMEOUT(len(p, "found") > 0, 40000);
        shot("sys-52-packages-search");
    }
    void packagesStreaming()
    {
        QObject *p = open("packages");
        if (!isRoot || !util("packages"))
            QSKIP("нужны root и менеджер пакетов");
        // через UI-метод install: диалог подтверждения, кнопка — как у пользователя
        js(p, "install", QVariantList{QStringList{"sherpa-no-such-package"}});
        QObject *btn = nullptr;
        QTRY_VERIFY_WITH_TIMEOUT((btn = child(p, "dangerConfirm")) && btn->property("enabled").toBool(), 5000);
        QMetaObject::invokeMethod(btn, "clicked");
        QTRY_VERIFY_WITH_TIMEOUT(!prop(p, "opId").toString().isEmpty(), 20000);
        QTRY_VERIFY_WITH_TIMEOUT(!prop(p, "opRunning").toBool(), 120000);
        QCOMPARE(prop(p, "opResult").toString(), QString("error"));   // пакета нет — ошибка пришла событием
        QVERIFY(!prop(p, "logText").toString().isEmpty());
        shot("sys-53-packages-operation");
    }

    // ---------------------------------------------------------------- Диски
    void disksWindow()
    {
        QObject *p = open("disks");
        QVERIFY(p);
        QTRY_VERIFY_WITH_TIMEOUT(len(p, "parts") > 0, 30000);
        for (const QVariant &d : p->property("parts").toList())
            QVERIFY(d.toMap().contains("usage_pct"));
        shot("sys-60-disks-partitions");
        // карта: скан каталога с известным содержимым
        // каталог создаётся локально; для внешнего sshd путь на его стороне задаёт SHERPA_EXT_SCAN_HOST (локально — SHERPA_EXT_SCAN_LOCAL)
        const QString local = qEnvironmentVariable("SHERPA_EXT_SCAN_LOCAL", env.dir.filePath("scan"));
        const QString root = qEnvironmentVariable("SHERPA_EXT_SCAN_HOST", local);
        QDir(local).removeRecursively();
        QDir().mkpath(local + "/a/deep");
        QDir().mkpath(local + "/b");
        QDir().mkpath(local + "/c");
        auto fill = [](const QString &path, int kb) { QFile f(path); f.open(QIODevice::WriteOnly); f.write(QByteArray(kb * 1024, 'x')); };
        fill(local + "/a/one.bin", 3000);
        fill(local + "/a/deep/two.bin", 1500);
        fill(local + "/a/deep/four.bin", 700);
        fill(local + "/b/three.bin", 1800);
        fill(local + "/c/five.bin", 900);
        fill(local + "/root.bin", 300);
        p->setProperty("tab", "map");
        js(p, "startScan", QVariantList{root});
        QTRY_VERIFY_WITH_TIMEOUT(p->property("tree").isValid() && !p->property("tree").isNull() && !prop(p, "scanning").toBool(), 60000);
        const QVariantMap tree = p->property("tree").toMap();
        QCOMPARE(tree.value("path").toString(), root);
        QVERIFY(tree.value("size").toDouble() > 8000.0 * 1024);
        QVariantList kids = tree.value("children").toList();
        QVERIFY(kids.size() >= 2);
        shot("sys-61-disks-treemap");
        p->setProperty("mapMode", "sunburst");
        shot("sys-62-disks-sunburst");
        p->setProperty("mapMode", "list");
        shot("sys-63-disks-list");
        // крупные файлы
        p->setProperty("tab", "big");
        p->setProperty("bigPath", root);
        p->setProperty("bigMin", "1");
        QMetaObject::invokeMethod(p, "runBig");
        QTRY_VERIFY_WITH_TIMEOUT(len(p, "big") >= 2, 60000);
        QCOMPARE(p->property("big").toList().first().toMap().value("path").toString(), root + "/a/one.bin");
        shot("sys-64-disks-large");
    }
    void windowsCoexist()
    {
        closeAll();
        for (const char *k : {"disks", "logs", "cron", "services", "packages", "system"})
            QVERIFY(open(k, false));
        QCOMPARE(page()->property("wins").toList().size(), 6);
        QTest::qWait(800);
        shot("sys-70-all-windows");
        // раскладка сохраняется и восстанавливается
        QTest::qWait(1000);
        QMetaObject::invokeMethod(win, "doCloseDesktop", Q_ARG(QVariant, connId));
        QTRY_VERIFY_WITH_TIMEOUT(be->desk()->desktops().isEmpty(), 5000);
        QMetaObject::invokeMethod(win, "openDesktop", Q_ARG(QVariant, connId), Q_ARG(QVariant, false));
        QTRY_VERIFY_WITH_TIMEOUT(desk() && desk()->state() == "ready", 30000);
        QTRY_COMPARE_WITH_TIMEOUT(page()->property("wins").toList().size(), 6, 10000);
    }
    void russian()
    {
        be->setLanguage("ru");
        closeAll();
        for (const char *k : {"system", "services"})
            open(k, false);
        QTest::qWait(2500);
        shot("sys-80-ru-system-services");
        be->setLanguage("en");
    }
    void cleanupTestCase()
    {
        if (be)
            be->client()->runBlocking({"daemon", "stop", "--force"});
        delete engine;
    }
};

QTEST_MAIN(TstSysUtils)
#include "tst_sysutils.moc"
