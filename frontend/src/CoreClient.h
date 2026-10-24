// Асинхронная обёртка над бинарником sherpa-core (QProcess). Ничего не знает о QML.
#pragma once

#include <QHash>
#include <QJsonObject>
#include <QObject>
#include <QProcess>
#include <QString>
#include <QStringList>
#include <QVariantList>
#include <QVariantMap>

struct CoreError {
    QString code;
    QString message;
    QVariantMap params;
    QVariantList details;
};

struct CoreResponse {
    bool ok = false;
    int exitCode = -1;
    QVariantMap data;
    CoreError error;
    QVariantList warnings;
    // true, если ядро не удалось запустить или оно вернуло не-JSON (error.code = client.*)
    bool transportFailure = false;
};
Q_DECLARE_METATYPE(CoreResponse)

class CoreClient : public QObject {
    Q_OBJECT
public:
    explicit CoreClient(QObject *parent = nullptr);
    ~CoreClient() override;

    // Путь к sherpa-core. Пусто — искать (SHERPA_CORE, рядом с приложением, PATH).
    void setProgram(const QString &path);
    QString program() const;
    // Глобальные флаги ядра (--db, --key-file, --config), идут перед командой.
    void setGlobalArgs(const QStringList &args) { m_globalArgs = args; }
    static QString locateCore();

    // Запустить команду. input (если не null) уходит в stdin как JSON с флагом --input -.
    quint64 run(const QStringList &args, const QJsonObject &input = {}, bool hasInput = false);
    // Потоковый режим (conn check --stream): streamEvent на каждую JSON-строку с "event",
    // finished — на итоговый конверт.
    quint64 runStream(const QStringList &args, const QJsonObject &input = {}, bool hasInput = false);
    // Прервать процесс; finished придёт с ошибкой client.cancelled.
    void cancel(quint64 id);
    // Для тестов и утилит: блокирующий вызов (внутри локальный цикл событий).
    CoreResponse runBlocking(const QStringList &args, const QJsonObject &input = {}, bool hasInput = false);

    static CoreResponse parseEnvelope(const QByteArray &line);

signals:
    void finished(quint64 id, const CoreResponse &response);
    void streamEvent(quint64 id, const QVariantMap &event);

private:
    struct Job {
        QProcess *proc = nullptr;
        bool stream = false;
        bool cancelled = false;
        bool done = false;
        QByteArray buf;
        CoreResponse final;
        bool haveFinal = false;
    };
    quint64 start(const QStringList &args, const QJsonObject &input, bool hasInput, bool stream);
    void consume(quint64 id, Job &job);
    void complete(quint64 id, int exitCode, bool crashed);
    static CoreResponse failure(const QString &code, const QString &message);

    QString m_program;
    QStringList m_globalArgs;
    quint64 m_nextId = 1;
    QHash<quint64, Job> m_jobs;
};
