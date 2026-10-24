// Запускает настоящий интерфейс offscreen на временной БД и снимает скриншоты экранов.
// Каталог снимков: SHERPA_SHOTS_DIR (по умолчанию ./screenshots).
#include "Backend.h"
#include "CoreEnv.h"

#include <QFontDatabase>
#include <QJsonArray>
#include <QQmlApplicationEngine>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QtQml/qqmlextensionplugin.h>
#include <QtTest>

Q_IMPORT_QML_PLUGIN(SherpaPlugin)

class TstScreens : public QObject {
    Q_OBJECT
    CoreEnv env;
    QQmlApplicationEngine *engine = nullptr;
    QQuickWindow *win = nullptr;
    Backend *be = nullptr;
    QString dir;

    void shot(const QString &name)
    {
        QTest::qWait(400);
        const QImage img = win->grabWindow();
        QVERIFY(!img.isNull());
        QVERIFY(img.save(dir + "/" + name + ".png"));
    }
    QObject *prop(const char *n) { return win->property(n).value<QObject *>(); }
    template <typename F> void waitFor(F f, int ms = 10000) { QTRY_VERIFY_WITH_TIMEOUT(f(), ms); }

private slots:
    void initTestCase()
    {
        dir = qEnvironmentVariable("SHERPA_SHOTS_DIR", QDir::currentPath() + "/screenshots");
        QDir().mkpath(dir);
        QQuickStyle::setStyle("Basic");
        QCoreApplication::setOrganizationName("SherpaTest");
        QCoreApplication::setApplicationName("SherpaTest");
        QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, env.dir.filePath("settings"));
        for (const char *f : {"JetBrainsMono-Regular.ttf", "JetBrainsMono-Bold.ttf", "Inter-Regular.otf",
                              "Inter-SemiBold.otf", "Inter-Bold.otf"})
            QFontDatabase::addApplicationFont(QStringLiteral(":/fonts/") + f);
        

        // ssh config для импорта
        QDir().mkpath(env.dir.filePath(".ssh"));
        QFile f(env.dir.filePath(".ssh/config"));
        QVERIFY(f.open(QIODevice::WriteOnly));
        f.write("Host *\n  User deploy\n\nHost build-1\n  HostName 10.1.0.5\n  IdentityFile ~/.ssh/id_ed25519\n\n"
                "Host build-2\n  HostName 10.1.0.6\n  Port 2222\n\nHost web-1\n  HostName 10.0.0.5\n");
        f.close();

        engine = new QQmlApplicationEngine;
        engine->loadFromModule("Sherpa", "Main");
        QVERIFY(!engine->rootObjects().isEmpty());
        win = qobject_cast<QQuickWindow *>(engine->rootObjects().first());
        QVERIFY(win);
        win->resize(1280, 800);
        be = engine->singletonInstance<Backend *>("Sherpa", "Backend");
        QVERIFY(be);
    }

    void firstRun()
    {
        waitFor([&] { return be->appState() == "firstrun"; });
        shot("01-first-run");
        be->finishFirstRun();
        waitFor([&] { return be->appState() == "ready"; });
    }
    void emptyHome()
    {
        waitFor([&] { return !be->loadingList(); });
        shot("02-home-empty");
    }
    void populated()
    {
        CoreClient *c = be->client();
        const auto mk = [&](const QString &name, const QString &host, int port, const QString &user, QStringList tags,
                            const QString &group = {}) {
            QJsonObject in{{"name", name}, {"host", host}, {"port", port}, {"user", user},
                           {"auth", QJsonObject{{"method", "password"}, {"password", "pw"}}}};
            QJsonArray t;
            for (auto &x : tags) t.append(x);
            in["tags"] = t;
            if (!group.isEmpty()) in["group_id"] = group;
            QVERIFY(c->runBlocking({"conn", "create"}, in, true).ok);
        };
        auto g = c->runBlocking({"group", "create", "--name", "Prod"});
        auto g2 = c->runBlocking({"group", "create", "--name", "Staging"});
        const QString gp = g.data.value("group").toMap().value("id").toString();
        const QString gs = g2.data.value("group").toMap().value("id").toString();
        mk("web-01", "127.0.0.1", 1, "root", {"prod", "nginx"}, gp);
        mk("web-02", "127.0.0.1", 2, "root", {"prod", "nginx"}, gp);
        mk("db-01", "127.0.0.1", 3, "postgres", {"prod", "db"}, gp);
        mk("stage-app", "localhost", 4, "deploy", {"staging"}, gs);
        mk("legacy", "192.0.2.10", 22, "admin", {});
        be->reload();
        waitFor([&] { return be->servers()->totalCount() == 5; });
        be->setSelectedId(be->servers()->visibleIds().first());
        shot("03-home-list");
    }
    void checkAllUpdatesStatuses()
    {
        be->checkAll();
        waitFor([&] { return !be->bulkChecking(); }, 30000);
        const auto st = be->stats();
        QCOMPARE(st.value("offline").toInt(), 5);
        shot("04-home-after-check-all");
    }
    void search()
    {
        be->setSearchText("web");
        QCOMPARE(be->servers()->rowCount(), 2);
        shot("05-home-search");
        be->setSearchText("zzzz");
        QCOMPARE(be->servers()->rowCount(), 0);
        shot("06-home-nothing-found");
        be->resetFilters();
        be->setTagFilter("db");
        QCOMPARE(be->servers()->rowCount(), 1);
        be->resetFilters();
    }
    void formCreate()
    {
        QMetaObject::invokeMethod(prop("formDialog"), "openCreate");
        shot("07-form-create");
        // валидация: пустая отправка
        QMetaObject::invokeMethod(prop("formDialog"), "submit", Q_ARG(QVariant, false));
        shot("08-form-validation");
        QMetaObject::invokeMethod(prop("formDialog"), "close");
    }
    void formEditAndDelete()
    {
        const QString id = be->servers()->visibleIds().first();
        QMetaObject::invokeMethod(prop("formDialog"), "openEdit", Q_ARG(QVariant, id));
        shot("09-form-edit");
        QMetaObject::invokeMethod(prop("formDialog"), "close");
        QMetaObject::invokeMethod(prop("home"), "deleteRequested", Q_ARG(QString, id));
        shot("10-confirm-delete");
        QVERIFY(QMetaObject::invokeMethod(prop("confirmDelete"), "close"));
        // дублирование через ядро
        be->duplicateConnection(id);
        waitFor([&] { return be->servers()->totalCount() == 6; });
    }
    void importFlow()
    {
        QObject *dlg = prop("importDialog");
        QMetaObject::invokeMethod(dlg, "openDialog");
        shot("11-import-path");
        QMetaObject::invokeMethod(dlg, "load");
        waitFor([&] { return dlg->property("step").toString() == "preview"; });
        shot("12-import-preview");
        QCOMPARE(dlg->property("selectedCount").toInt(), 3);
        be->importApply({{"path", env.dir.filePath(".ssh/config")},
                         {"previewId", dlg->property("preview").toMap().value("preview_id")},
                         {"select", QStringList{"build-1", "build-2"}}});
        waitFor([&] { return dlg->property("step").toString() == "report"; });
        shot("13-import-report");
        QMetaObject::invokeMethod(dlg, "close");
        waitFor([&] { return be->servers()->totalCount() == 8; });
    }
    void hostKeyDialogs()
    {
        const QString id = be->servers()->visibleIds().first();
        be->injectHostKey({{"connId", id}, {"name", "web-01"}, {"kind", "unknown"}, {"host", "10.0.0.5"}, {"port", 22},
                           {"keyType", "ssh-ed25519"}, {"fingerprint", "SHA256:Qm9vYmFyQmF6UXV4MTIzNDU2Nzg5MGFiY2RlZmdoaWo"}});
        shot("14-hostkey-new");
        be->answerHostKey("cancel");
        be->injectHostKey({{"connId", id}, {"name", "web-01"}, {"kind", "mismatch"}, {"host", "10.0.0.5"}, {"port", 22},
                           {"keyType", "ssh-ed25519"}, {"fingerprint", "SHA256:ZXZpbGV2aWxldmlsZXZpbGV2aWxldmlsZXZpbGV2aWw"},
                           {"knownFingerprint", "SHA256:Qm9vYmFyQmF6UXV4MTIzNDU2Nzg5MGFiY2RlZmdoaWo"},
                           {"knownAcceptedAt", "2026-10-01T09:15:00Z"}});
        shot("15-hostkey-changed");
        be->answerHostKey("cancel");
    }
    void settingsAndLanguage()
    {
        QMetaObject::invokeMethod(prop("settingsDialog"), "open");
        shot("16-settings");
        QMetaObject::invokeMethod(prop("settingsDialog"), "close");
        be->setLanguage("ru");
        QTest::qWait(500);
        shot("17-home-ru");
        QMetaObject::invokeMethod(prop("formDialog"), "openCreate");
        shot("18-form-ru");
        QMetaObject::invokeMethod(prop("formDialog"), "close");
        QMetaObject::invokeMethod(prop("settingsDialog"), "open");
        shot("19-settings-ru");
        QMetaObject::invokeMethod(prop("settingsDialog"), "close");
        be->setTheme("light");
        shot("20-home-light");
        be->setTheme("dark");
        be->setLanguage("en");
    }
    void cleanupTestCase() { delete engine; }
};

QTEST_MAIN(TstScreens)
#include "tst_screens.moc"
