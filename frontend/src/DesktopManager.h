// Рабочие столы серверов: desktop.open, мониторинг (monitor.subscribe), раскладка окон, терминалы.
#pragma once

#include "DaemonClient.h"
#include "TerminalSession.h"

#include <QHash>
#include <QJSValue>
#include <QObject>
#include <QTimer>
#include <QVariantMap>
#include <QtQml/qqmlregistration.h>

class DesktopManager;

class DesktopState : public QObject {
    Q_OBJECT
    QML_ELEMENT
    QML_UNCREATABLE("created by DesktopManager")
    Q_PROPERTY(QString connectionId READ connectionId CONSTANT)
    Q_PROPERTY(QString name READ name NOTIFY nameChanged)
    // connecting | ready | error | lost
    Q_PROPERTY(QString state READ state NOTIFY stateChanged)
    Q_PROPERTY(QString errorText READ errorText NOTIFY stateChanged)
    Q_PROPERTY(QString errorCode READ errorCode NOTIFY stateChanged)
    Q_PROPERTY(int retryIn READ retryIn NOTIFY retryChanged)
    Q_PROPERTY(QVariantMap capabilities READ capabilities NOTIFY capabilitiesChanged)
    Q_PROPERTY(QVariantMap monitor READ monitor NOTIFY monitorChanged)
    Q_PROPERTY(QVariantList cpuHistory READ cpuHistory NOTIFY monitorChanged)
    Q_PROPERTY(QVariantList memHistory READ memHistory NOTIFY monitorChanged)
    Q_PROPERTY(QVariantList loadHistory READ loadHistory NOTIFY monitorChanged)
    Q_PROPERTY(bool readOnlyMode READ readonlyMode NOTIFY readonlyChanged)
    Q_PROPERTY(QVariantMap prompt READ prompt NOTIFY promptChanged)
    Q_PROPERTY(QVariantMap layout READ layout NOTIFY layoutLoaded)
    Q_PROPERTY(bool layoutReady READ layoutReady NOTIFY layoutLoaded)
public:
    DesktopState(DesktopManager *mgr, DaemonClient *daemon, const QString &id, const QString &name);

    QString connectionId() const { return m_id; }
    QString name() const { return m_name; }
    void setName(const QString &n);
    QString state() const { return m_state; }
    QString errorText() const { return m_errorText; }
    QString errorCode() const { return m_errorCode; }
    int retryIn() const { return m_retryIn; }
    QVariantMap capabilities() const { return m_caps; }
    QVariantMap monitor() const { return m_monitor; }
    QVariantList cpuHistory() const { return m_cpu; }
    QVariantList memHistory() const { return m_mem; }
    QVariantList loadHistory() const { return m_load; }
    bool readonlyMode() const { return m_readonly; }
    QVariantMap prompt() const { return m_prompt; }
    QVariantMap layout() const { return m_layout; }
    bool layoutReady() const { return m_layoutReady; }

    Q_INVOKABLE void open();
    Q_INVOKABLE void retryNow();
    Q_INVOKABLE void close();
    Q_INVOKABLE void answerHostKey(const QString &decision);
    Q_INVOKABLE void answerAuth(const QString &response, bool save);
    Q_INVOKABLE void cancelAuth();
    Q_INVOKABLE void setReadonly(bool on);
    // Раскладка: desktop.state.get/set если ядро умеет, иначе локальный файл.
    Q_INVOKABLE void loadLayout();
    Q_INVOKABLE void saveLayout(const QVariantMap &layout);
    Q_INVOKABLE TerminalSession *newTerminal();
    // Мини-запрос к демону из QML (для утилит): method, params → сигнал requestDone
    Q_INVOKABLE int call(const QString &method, const QVariantMap &params);
    // Запрос к демону с обратным вызовом: cb(ok, data, errorCode, errorText, errorParams). connection_id подставляется сам.
    Q_INVOKABLE void rpc(const QString &method, const QVariantMap &params, const QJSValue &cb);
    // То же без connection_id (transfer.cancel, tunnel.stop, danger.prepare…)
    Q_INVOKABLE void rpcRaw(const QString &method, const QVariantMap &params, const QJSValue &cb);
    bool hasMethod(const QString &m) const { return m_daemon->hasMethod(m); }

    void handleEvent(const QVariantMap &ev);
    void onDaemonLost();
    void onDaemonBack();
    void shutdown();

signals:
    void nameChanged();
    void stateChanged();
    void retryChanged();
    void capabilitiesChanged();
    void monitorChanged();
    void readonlyChanged();
    void promptChanged();
    void layoutLoaded();
    void terminalCreated(TerminalSession *session);
    // Любое событие демона этого сервера (transfer, tunnel, log…) — для окон утилит
    void eventReceived(const QVariantMap &ev);
    void requestDone(int token, bool ok, const QVariant &data, const QString &errorText);

private:
    void setState(const QString &s);
    void subscribeMonitor();
    void markLost(const QString &code, const QString &text);
    void startRetry();
    QString layoutPath() const;

    DesktopManager *m_mgr;
    DaemonClient *m_daemon;
    QString m_id, m_name, m_state = "connecting", m_errorText, m_errorCode, m_subscription;
    QVariantMap m_caps, m_monitor, m_prompt, m_layout;
    QVariantList m_cpu, m_mem, m_load;
    bool m_readonly = false, m_layoutReady = false, m_closing = false, m_opening = false;
    int m_retryIn = 0, m_retryAttempt = 0, m_nextToken = 1;
    QTimer m_retryTimer;
};

class DesktopManager : public QObject {
    Q_OBJECT
    QML_ELEMENT
    QML_UNCREATABLE("owned by Backend")
    Q_PROPERTY(QVariantList desktops READ desktops NOTIFY desktopsChanged)
    // Только идентификаторы открытых столов: меняется при открытии/закрытии, но не при смене состояния
    Q_PROPERTY(QStringList ids READ ids NOTIFY idsChanged)
    Q_PROPERTY(QString daemonState READ daemonState NOTIFY daemonStateChanged)
    Q_PROPERTY(QString daemonError READ daemonError NOTIFY daemonStateChanged)
public:
    DesktopManager(DaemonClient *daemon, QObject *parent = nullptr);

    QVariantList desktops() const;
    QStringList ids() const { return m_order; }
    QString daemonState() const;
    QString daemonError() const { return m_daemon->lastError(); }
    DaemonClient *daemon() { return m_daemon; }

    // Открыть (или вернуть уже открытый) рабочий стол.
    Q_INVOKABLE DesktopState *openDesktop(const QString &connectionId, const QString &name);
    Q_INVOKABLE DesktopState *desktop(const QString &connectionId) const { return m_desktops.value(connectionId); }
    Q_INVOKABLE void closeDesktop(const QString &connectionId);
    Q_INVOKABLE void startDaemon() { m_daemon->start(); }

    void registerSession(TerminalSession *s);

signals:
    void desktopsChanged();
    void idsChanged();
    void daemonStateChanged();
    void desktopOpened(const QString &connectionId);
    void desktopClosed(const QString &connectionId);

private:
    void onEvent(const QVariantMap &ev);

    DaemonClient *m_daemon;
    QHash<QString, DesktopState *> m_desktops;
    QStringList m_order;
    QHash<QString, TerminalSession *> m_sessions;
    QHash<QString, QList<QVariantMap>> m_orphans; // события сессии, пришедшие раньше регистрации
};
