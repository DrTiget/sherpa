// Терминальная сессия демона (session.*): эмулятор + состояние + вопросы host key / пароль.
// Живёт независимо от окна: вывод копится в эмуляторе, пока окно свёрнуто или закрыто.
#pragma once

#include "DaemonClient.h"
#include "TermEmulator.h"

#include <QObject>
#include <QVariantMap>
#include <QtQml/qqmlregistration.h>

class TerminalSession : public QObject {
    Q_OBJECT
    QML_ELEMENT
    QML_UNCREATABLE("created by DesktopManager")
    Q_PROPERTY(QString sessionId READ sessionId NOTIFY sessionIdChanged)
    Q_PROPERTY(QString connectionId READ connectionId CONSTANT)
    // connecting | awaiting | connected | lost | closed | error
    Q_PROPERTY(QString state READ state NOTIFY stateChanged)
    Q_PROPERTY(QString title READ title NOTIFY titleChanged)
    Q_PROPERTY(QString banner READ banner NOTIFY stateChanged)
    Q_PROPERTY(QString errorText READ errorText NOTIFY stateChanged)
    Q_PROPERTY(QString errorCode READ errorCode NOTIFY stateChanged)
    Q_PROPERTY(QString reason READ reason NOTIFY stateChanged)
    Q_PROPERTY(QVariantMap prompt READ prompt NOTIFY promptChanged)
    Q_PROPERTY(TermEmulator *emulator READ emulator CONSTANT)
    Q_PROPERTY(bool active READ active NOTIFY stateChanged)
public:
    TerminalSession(DaemonClient *daemon, const QString &connectionId, QObject *parent = nullptr);

    QString sessionId() const { return m_id; }
    QString connectionId() const { return m_connectionId; }
    QString state() const { return m_state; }
    QString title() const;
    QString banner() const { return m_banner; }
    QString errorText() const { return m_errorText; }
    QString errorCode() const { return m_errorCode; }
    QString reason() const { return m_reason; }
    QVariantMap prompt() const { return m_prompt; }
    TermEmulator *emulator() { return &m_emu; }
    bool active() const { return m_state == "connecting" || m_state == "awaiting" || m_state == "connected"; }

    // Открыть (или открыть заново) сессию.
    Q_INVOKABLE void open(int cols = 80, int rows = 24);
    Q_INVOKABLE void sendInput(const QByteArray &bytes);
    Q_INVOKABLE void resize(int cols, int rows);
    Q_INVOKABLE void close();
    // Новая сессия к тому же серверу после обрыва/выхода.
    Q_INVOKABLE void reconnect();
    Q_INVOKABLE void answerHostKey(const QString &decision);
    Q_INVOKABLE void answerAuth(const QString &response, bool save);
    Q_INVOKABLE void cancelAuth();

    // Вызывается DesktopManager
    void handleEvent(const QVariantMap &ev);
    void onDaemonLost();
    void onDaemonBack();
    void setSecrets(const QVariantMap &s) { m_secrets = s; }

signals:
    void sessionIdChanged();
    void stateChanged();
    void titleChanged();
    void promptChanged();
    void opened(const QString &sessionId);   // сессия получила id (DesktopManager регистрирует)
    void bell();

private:
    void sendResize();
    void setState(const QString &s);
    void fail(const CoreError &e);

    DaemonClient *m_daemon;
    QString m_connectionId, m_id, m_state = "closed", m_banner, m_errorText, m_errorCode, m_reason;
    QVariantMap m_prompt, m_secrets;
    TermEmulator m_emu;
    int m_cols = 80, m_rows = 24;
    bool m_opening = false;
    bool m_replayFresh = false;
    bool m_resizeInFlight = false, m_resizePending = false;
};
