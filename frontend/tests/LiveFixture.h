// Общая обвязка для сквозных тестов окон: локальный sshd, настоящий демон ядра, весь интерфейс offscreen.
#pragma once

#include "Backend.h"
#include "CoreEnv.h"
#include "TestSshd.h"

#include <QFontDatabase>
#include <QJsonArray>
#include <QQmlApplicationEngine>
#include <QQuickItem>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QSettings>
#include <QStandardPaths>
#include <QtTest>

#define QTRY_VERIFY2_WITH_TIMEOUT_RET(cond, msg, ms, ret) \
    do { QTRY_VERIFY2_WITH_TIMEOUT_IMPL(cond, msg, ms, ret) } while (0)
#define QTRY_VERIFY2_WITH_TIMEOUT_IMPL(cond, msg, ms, ret) \
    { QElapsedTimer t_; t_.start(); while (!(cond) && t_.elapsed() < (ms)) QTest::qWait(50); if (!(cond)) { qWarning() << "timeout:" << msg; return ret; } }
#define QTRY_COMPARE_WITH_TIMEOUT_RET(a, b, ms, ret) \
    { QElapsedTimer t_; t_.start(); while (!((a) == (b)) && t_.elapsed() < (ms)) QTest::qWait(50); if (!((a) == (b))) { qWarning() << "timeout compare" << (a) << (b); return ret; } }

struct LiveFixture {
    CoreEnv env;
    TestSshd sshd;
    QQmlApplicationEngine *engine = nullptr;
    QQuickWindow *win = nullptr;
    Backend *be = nullptr;
    QString shotsDir, serverId;

    // false — нет sshd (тест пропускается)
    bool init(const QString &appName)
    {
        if (TestSshd::find().isEmpty())
            return false;
        if (!sshd.start())
            return false;
        qputenv("XDG_RUNTIME_DIR", env.dir.filePath("run").toUtf8());
        QDir().mkpath(env.dir.filePath("run"));
        QFile(env.dir.filePath("run")).setPermissions(QFile::ReadOwner | QFile::WriteOwner | QFile::ExeOwner);
        shotsDir = qEnvironmentVariable("SHERPA_SHOTS_DIR", QDir::currentPath() + "/screenshots");
        QDir().mkpath(shotsDir);
        QQuickStyle::setStyle("Basic");
        QCoreApplication::setOrganizationName("SherpaTest");
        QCoreApplication::setApplicationName(appName);
        QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, env.dir.filePath("settings"));
        for (const char *f : {"JetBrainsMono-Regular.ttf", "JetBrainsMono-Bold.ttf", "Inter-Regular.otf", "Inter-SemiBold.otf", "Inter-Bold.otf"})
            QFontDatabase::addApplicationFont(QStringLiteral(":/fonts/") + f);
        engine = new QQmlApplicationEngine;
        engine->loadFromModule("Sherpa", "Main");
        if (engine->rootObjects().isEmpty())
            return false;
        win = qobject_cast<QQuickWindow *>(engine->rootObjects().first());
        win->resize(1360, 840);
        be = engine->singletonInstance<Backend *>("Sherpa", "Backend");
        return true;
    }
    // первый запуск пройден, сервер создан и host key принят заранее
    bool prepareServer()
    {
        QTRY_VERIFY2_WITH_TIMEOUT_RET(be->appState() == "firstrun", "firstrun", 10000, false);
        be->finishFirstRun();
        QTRY_VERIFY2_WITH_TIMEOUT_RET(be->appState() == "ready", "ready", 10000, false);
        QJsonObject in{{"name", "files-srv"}, {"host", "127.0.0.1"}, {"port", sshd.port}, {"user", sshd.user},
                       {"auth", QJsonObject{{"method", "key_text"}, {"key_text", sshd.readKey(sshd.clientKeyPath)}}}};
        auto r = be->client()->runBlocking({"conn", "create"}, in, true);
        serverId = r.data.value("connection").toMap().value("id").toString();
        be->client()->runBlocking({"conn", "check", serverId, "--accept-new"});
        be->reload();
        QTRY_COMPARE_WITH_TIMEOUT_RET(be->servers()->totalCount(), 1, 10000, false);
        return !serverId.isEmpty();
    }
    bool openDesktop(bool terminal = false)
    {
        QMetaObject::invokeMethod(win, "openDesktop", Q_ARG(QVariant, serverId), Q_ARG(QVariant, terminal));
        DesktopState *d = desk();
        QTRY_VERIFY2_WITH_TIMEOUT_RET(d && d->state() == "ready", "desktop ready", 30000, false);
        return true;
    }
    DesktopState *desk() { return be->desk()->desktop(serverId); }
    QObject *page()
    {
        QObject *rep = win->property("desktopRepeater").value<QObject *>();
        QQuickItem *p = nullptr;
        QMetaObject::invokeMethod(rep, "itemAt", Q_RETURN_ARG(QQuickItem *, p), Q_ARG(int, 0));
        return p;
    }
    QVariantList wins() { return page()->property("wins").toList(); }
    // Открыть окно утилиты и вернуть корневой объект её содержимого
    QObject *openWindow(const QString &kind)
    {
        QMetaObject::invokeMethod(page(), "openWindow", Q_ARG(QVariant, kind), Q_ARG(QVariant, QVariant()),
                                  Q_ARG(QVariant, QVariant()), Q_ARG(QVariant, QVariant()));
        return content(kind);
    }
    QObject *content(const QString &kind, int nth = 0)
    {
        int n = 0;
        for (const QVariant &w : wins()) {
            const QVariantMap m = w.toMap();
            if (m.value("kind") == kind && n++ == nth)
                return m.value("content").value<QObject *>();
        }
        return nullptr;
    }
    QObject *windowOf(const QString &kind, int nth = 0)
    {
        int n = 0;
        for (const QVariant &w : wins()) {
            const QVariantMap m = w.toMap();
            if (m.value("kind") == kind && n++ == nth)
                return m.value("win").value<QObject *>();
        }
        return nullptr;
    }
    void shot(const QString &name)
    {
        QTest::qWait(500);
        const QImage img = win->grabWindow();
        QVERIFY(!img.isNull());
        QVERIFY(img.save(shotsDir + "/" + name + ".png"));
    }
    ~LiveFixture()
    {
        if (be)
            be->client()->runBlocking({"daemon", "stop", "--force"});
        delete engine;
    }
};

