#include "TerminalSession.h"

#include "ErrorText.h"

#include <QCoreApplication>

TerminalSession::TerminalSession(DaemonClient *daemon, const QString &connectionId, QObject *parent)
    : QObject(parent), m_daemon(daemon), m_connectionId(connectionId), m_emu(80, 24)
{
    connect(&m_emu, &TermEmulator::response, this, &TerminalSession::sendInput);
    connect(&m_emu, &TermEmulator::titleChanged, this, &TerminalSession::titleChanged);
    connect(&m_emu, &TermEmulator::bell, this, &TerminalSession::bell);
}

QString TerminalSession::title() const { return m_emu.title(); }

void TerminalSession::setState(const QString &s)
{
    if (m_state == s)
        return;
    m_state = s;
    emit stateChanged();
}

void TerminalSession::open(int cols, int rows)
{
    if (m_opening || active())
        return;
    m_cols = qMax(1, cols);
    m_rows = qMax(1, rows);
    m_opening = true;
    m_errorText.clear();
    m_errorCode.clear();
    m_reason.clear();
    setState("connecting");
    QVariantMap p{{"connection_id", m_connectionId}, {"cols", m_cols}, {"rows", m_rows}, {"term", "xterm-256color"}};
    if (!m_secrets.isEmpty())
        p.insert("secrets", m_secrets);
    m_daemon->request("session.open", p, [this](const DaemonReply &r) {
        m_opening = false;
        if (!r.ok) {
            fail(r.error);
            return;
        }
        m_id = r.data.toMap().value("session").toString();
        emit sessionIdChanged();
        emit opened(m_id);
    });
}

void TerminalSession::fail(const CoreError &e)
{
    m_errorCode = e.code;
    m_errorText = ErrorText::text(e.code, e.params, e.message);
    m_prompt.clear();
    emit promptChanged();
    setState("error");
    m_emu.notice(m_errorText, 1);
}

void TerminalSession::sendInput(const QByteArray &bytes)
{
    if (m_state != "connected" || m_id.isEmpty() || bytes.isEmpty())
        return;
    m_daemon->request("session.input", {{"session", m_id}, {"data_b64", QString::fromLatin1(bytes.toBase64())}});
}

void TerminalSession::resize(int cols, int rows)
{
    if (cols < 1 || rows < 1)
        return;
    m_cols = cols;
    m_rows = rows;
    m_emu.resize(cols, rows);
    if (m_id.isEmpty() || !active())
        return;
    m_daemon->request("session.resize", {{"session", m_id}, {"cols", cols}, {"rows", rows}});
}

void TerminalSession::close()
{
    if (m_id.isEmpty() || !active()) {
        setState("closed");
        return;
    }
    m_daemon->request("session.close", {{"session", m_id}});
}

void TerminalSession::reconnect()
{
    if (active())
        return;
    m_emu.notice(QCoreApplication::translate("Terminal", "Reconnecting…"), 6);
    m_id.clear();
    emit sessionIdChanged();
    open(m_cols, m_rows);
}

void TerminalSession::answerHostKey(const QString &decision)
{
    if (m_prompt.isEmpty())
        return;
    QVariantMap p{{"prompt_id", m_prompt.value("prompt_id")}, {"decision", decision}};
    if (!m_id.isEmpty())
        p.insert("session", m_id);
    m_prompt.clear();
    emit promptChanged();
    m_daemon->request("session.hostkey_answer", p);
}

void TerminalSession::answerAuth(const QString &response, bool save)
{
    if (m_prompt.isEmpty())
        return;
    QVariantMap p{{"prompt_id", m_prompt.value("prompt_id")}, {"responses", QStringList{response}}, {"save", save}};
    if (!m_id.isEmpty())
        p.insert("session", m_id);
    m_prompt.clear();
    emit promptChanged();
    m_daemon->request("session.auth_answer", p);
}

void TerminalSession::cancelAuth()
{
    if (m_prompt.isEmpty())
        return;
    QVariantMap p{{"prompt_id", m_prompt.value("prompt_id")}, {"cancel", true}};
    if (!m_id.isEmpty())
        p.insert("session", m_id);
    m_prompt.clear();
    emit promptChanged();
    m_daemon->request("session.auth_answer", p);
}

void TerminalSession::handleEvent(const QVariantMap &ev)
{
    const QString name = ev.value("event").toString();
    if (name == "connecting") {
        setState("connecting");
    } else if (name == "hostkey_prompt" || name == "auth_prompt") {
        m_prompt = ev;
        m_prompt.insert("type", name == "hostkey_prompt" ? "hostkey" : "auth");
        emit promptChanged();
        setState("awaiting");
    } else if (name == "connected") {
        m_banner = ev.value("server_banner").toString();
        m_prompt.clear();
        emit promptChanged();
        setState("connected");
        // сообщить серверу актуальный размер (мог измениться пока подключались)
        m_daemon->request("session.resize", {{"session", m_id}, {"cols", m_cols}, {"rows", m_rows}});
    } else if (name == "output") {
        if (ev.value("replay").toBool() && m_replayFresh) {
            m_emu.reset();
            m_replayFresh = false;
        }
        m_emu.feed(QByteArray::fromBase64(ev.value("data_b64").toString().toLatin1()));
    } else if (name == "closed") {
        m_reason = ev.value("reason").toString();
        m_prompt.clear();
        emit promptChanged();
        if (m_state != "error")
            setState("closed");
        else
            emit stateChanged();
        if (m_errorCode.isEmpty() && m_reason != "client")
            m_emu.notice(QCoreApplication::translate("Terminal", "Session ended"), 3);
    } else if (name == "error") {
        const QVariantMap e = ev.value("error").toMap();
        CoreError ce;
        ce.code = e.value("code").toString();
        ce.message = e.value("message").toString();
        ce.params = e.value("params").toMap();
        if (ev.value("fatal").toBool())
            fail(ce);
        else
            m_emu.notice(ErrorText::text(ce.code, ce.params, ce.message), 1);
    }
}

void TerminalSession::onDaemonLost()
{
    if (!active())
        return;
    m_prompt.clear();
    emit promptChanged();
    setState("lost");
    m_emu.notice(QCoreApplication::translate("Terminal", "Connection to the core lost. Reconnecting…"), 1);
}

void TerminalSession::onDaemonBack()
{
    if (m_state != "lost" || m_id.isEmpty())
        return;
    if (!m_daemon->knownSessions().contains(m_id)) {
        m_reason = "remote";
        setState("closed");
        m_emu.notice(QCoreApplication::translate("Terminal", "Session ended"), 3);
        return;
    }
    m_replayFresh = true;
    m_daemon->request("session.attach", {{"session", m_id}}, [this](const DaemonReply &r) {
        if (!r.ok) {
            m_replayFresh = false;
            m_reason = "remote";
            setState("closed");
            return;
        }
        setState("connected");
        m_daemon->request("session.resize", {{"session", m_id}, {"cols", m_cols}, {"rows", m_rows}});
    });
}
