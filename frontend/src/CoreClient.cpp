#include "CoreClient.h"

#include <QCoreApplication>
#include <QDir>
#include <QEventLoop>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QStandardPaths>
#include <QTimer>

namespace {
#ifdef Q_OS_WIN
const char *kBinary = "sherpa-core.exe";
#else
const char *kBinary = "sherpa-core";
#endif

CoreError parseError(const QJsonObject &o)
{
    CoreError e;
    e.code = o.value("code").toString();
    e.message = o.value("message").toString();
    e.params = o.value("params").toObject().toVariantMap();
    e.details = o.value("details").toArray().toVariantList();
    return e;
}
} // namespace

CoreClient::CoreClient(QObject *parent) : QObject(parent)
{
    qRegisterMetaType<CoreResponse>("CoreResponse");
}

CoreClient::~CoreClient()
{
    for (auto it = m_jobs.begin(); it != m_jobs.end(); ++it) {
        if (it->proc) {
            it->proc->disconnect(this);
            it->proc->kill();
            it->proc->waitForFinished(500);
        }
    }
}

void CoreClient::setProgram(const QString &path) { m_program = path; }

QString CoreClient::program() const { return m_program.isEmpty() ? locateCore() : m_program; }

QString CoreClient::locateCore()
{
    const QString env = qEnvironmentVariable("SHERPA_CORE");
    if (!env.isEmpty())
        return env;
    const QString beside = QDir(QCoreApplication::applicationDirPath()).filePath(kBinary);
    if (QFileInfo::exists(beside))
        return beside;
    const QString inPath = QStandardPaths::findExecutable(kBinary);
    return inPath.isEmpty() ? QString::fromLatin1(kBinary) : inPath;
}

CoreResponse CoreClient::failure(const QString &code, const QString &message)
{
    CoreResponse r;
    r.transportFailure = true;
    r.error.code = code;
    r.error.message = message;
    return r;
}

CoreResponse CoreClient::parseEnvelope(const QByteArray &line)
{
    QJsonParseError pe;
    const QJsonDocument doc = QJsonDocument::fromJson(line, &pe);
    if (pe.error != QJsonParseError::NoError || !doc.isObject())
        return failure("client.bad_output", QStringLiteral("Core returned invalid JSON"));
    const QJsonObject o = doc.object();
    CoreResponse r;
    r.ok = o.value("ok").toBool();
    r.warnings = o.value("warnings").toArray().toVariantList();
    if (r.ok) {
        const QJsonValue d = o.value("data");
        if (d.isObject())
            r.data = d.toObject().toVariantMap();
    } else {
        r.error = parseError(o.value("error").toObject());
        if (r.error.code.isEmpty())
            r.error.code = "internal";
    }
    return r;
}

quint64 CoreClient::run(const QStringList &args, const QJsonObject &input, bool hasInput)
{
    return start(args, input, hasInput, false);
}

quint64 CoreClient::runStream(const QStringList &args, const QJsonObject &input, bool hasInput)
{
    return start(args, input, hasInput, true);
}

quint64 CoreClient::start(const QStringList &args, const QJsonObject &input, bool hasInput, bool stream)
{
    const quint64 id = m_nextId++;
    Job &job = m_jobs[id];
    job.stream = stream;
    job.proc = new QProcess(this);
    QProcess *p = job.proc;

    QStringList full = m_globalArgs;
    full << args;
    if (hasInput)
        full << "--input" << "-";

    connect(p, &QProcess::readyReadStandardOutput, this, [this, id]() {
        auto it = m_jobs.find(id);
        if (it == m_jobs.end())
            return;
        it->buf += it->proc->readAllStandardOutput();
        consume(id, *it);
    });
    connect(p, &QProcess::finished, this, [this, id](int code, QProcess::ExitStatus st) {
        complete(id, code, st == QProcess::CrashExit);
    });
    connect(p, &QProcess::errorOccurred, this, [this, id](QProcess::ProcessError err) {
        if (err == QProcess::FailedToStart) {
            auto it = m_jobs.find(id);
            if (it == m_jobs.end() || it->done)
                return;
            it->final = failure("client.core_not_found",
                                QStringLiteral("Cannot start %1").arg(it->proc->program()));
            it->haveFinal = true;
            // finished не должен прийти раньше, чем вызывающий успеет сохранить id
            QMetaObject::invokeMethod(this, [this, id]() { complete(id, -1, false); }, Qt::QueuedConnection);
        }
    });

    p->setProcessChannelMode(QProcess::SeparateChannels);
    p->start(program(), full);
    if (hasInput) {
        p->write(QJsonDocument(input).toJson(QJsonDocument::Compact));
        p->closeWriteChannel();
    } else {
        p->closeWriteChannel();
    }
    return id;
}

void CoreClient::consume(quint64 id, Job &job)
{
    int nl;
    while ((nl = job.buf.indexOf('\n')) >= 0) {
        const QByteArray line = job.buf.left(nl).trimmed();
        job.buf.remove(0, nl + 1);
        if (line.isEmpty())
            continue;
        if (job.stream) {
            QJsonParseError pe;
            const QJsonDocument doc = QJsonDocument::fromJson(line, &pe);
            if (pe.error == QJsonParseError::NoError && doc.isObject()
                && doc.object().contains("event")) {
                emit streamEvent(id, doc.object().toVariantMap());
                if (!m_jobs.contains(id))
                    return;
                continue;
            }
        }
        job.final = parseEnvelope(line);
        job.haveFinal = true;
    }
}

void CoreClient::complete(quint64 id, int exitCode, bool crashed)
{
    auto it = m_jobs.find(id);
    if (it == m_jobs.end() || it->done)
        return;
    Job &job = *it;
    job.done = true;
    if (job.proc && job.proc->state() != QProcess::NotRunning)
        job.proc->waitForFinished(200);
    if (job.proc)
        job.buf += job.proc->readAllStandardOutput();
    consume(id, job);
    if (!m_jobs.contains(id))
        return;
    CoreResponse r;
    if (job.cancelled) {
        r = failure("client.cancelled", QStringLiteral("Cancelled"));
    } else if (job.haveFinal) {
        r = job.final;
    } else if (crashed) {
        r = failure("client.process_failed", QStringLiteral("Core crashed"));
    } else {
        r = failure("client.bad_output", QStringLiteral("Core returned no result"));
    }
    r.exitCode = exitCode;
    QProcess *p = job.proc;
    m_jobs.erase(it);
    if (p)
        p->deleteLater();
    emit finished(id, r);
}

void CoreClient::cancel(quint64 id)
{
    auto it = m_jobs.find(id);
    if (it == m_jobs.end() || it->done)
        return;
    it->cancelled = true;
    it->proc->kill();
}

CoreResponse CoreClient::runBlocking(const QStringList &args, const QJsonObject &input, bool hasInput)
{
    QEventLoop loop;
    CoreResponse result;
    const quint64 id = run(args, input, hasInput);
    auto c = connect(this, &CoreClient::finished, &loop, [&](quint64 fid, const CoreResponse &r) {
        if (fid == id) {
            result = r;
            loop.quit();
        }
    });
    loop.exec();
    disconnect(c);
    return result;
}
