// Мост между QML и CoreClient: состояние приложения, списки, проверки, host key, импорт, настройки.
#pragma once

#include "CoreClient.h"
#include "ServerModel.h"

#include <QHash>
#include <QObject>
#include <QSettings>
#include <QTranslator>
#include <QVariantList>
#include <QVariantMap>
#include <QtQml/qqmlregistration.h>
#include <functional>

class Backend : public QObject {
    Q_OBJECT
    QML_ELEMENT
    QML_SINGLETON

    // "loading" | "firstrun" | "ready" | "fatal"
    Q_PROPERTY(QString appState READ appState NOTIFY appStateChanged)
    Q_PROPERTY(ServerModel *servers READ servers CONSTANT)
    Q_PROPERTY(QVariantList groups READ groups NOTIFY groupsChanged)
    Q_PROPERTY(QVariantList tags READ tags NOTIFY tagsChanged)
    Q_PROPERTY(QString searchText READ searchText WRITE setSearchText NOTIFY filterChanged)
    // "" — все, "__none__" — без группы, иначе id группы
    Q_PROPERTY(QString groupFilter READ groupFilter WRITE setGroupFilter NOTIFY filterChanged)
    Q_PROPERTY(QString tagFilter READ tagFilter WRITE setTagFilter NOTIFY filterChanged)
    Q_PROPERTY(QString selectedId READ selectedId WRITE setSelectedId NOTIFY selectedChanged)
    Q_PROPERTY(QVariantMap selected READ selected NOTIFY selectedChanged)
    Q_PROPERTY(bool loadingList READ loadingList NOTIFY loadingListChanged)
    Q_PROPERTY(bool bulkChecking READ bulkChecking NOTIFY checkingChanged)
    Q_PROPERTY(bool anyChecking READ anyChecking NOTIFY checkingChanged)
    Q_PROPERTY(int checkDone READ checkDone NOTIFY checkingChanged)
    Q_PROPERTY(int checkTotal READ checkTotal NOTIFY checkingChanged)
    Q_PROPERTY(QVariantMap stats READ stats NOTIFY statsChanged)
    Q_PROPERTY(QVariantMap banner READ banner NOTIFY bannerChanged)
    Q_PROPERTY(bool backupPending READ backupPending NOTIFY backupPendingChanged)
    Q_PROPERTY(QString keyPath READ keyPath NOTIFY pathsChanged)
    Q_PROPERTY(QString dbPath READ dbPath NOTIFY pathsChanged)
    Q_PROPERTY(QString coreVersion READ coreVersion NOTIFY pathsChanged)
    Q_PROPERTY(QString fatalText READ fatalText NOTIFY appStateChanged)
    Q_PROPERTY(QString fatalCode READ fatalCode NOTIFY appStateChanged)
    Q_PROPERTY(QVariantMap pendingHostKey READ pendingHostKey NOTIFY pendingHostKeyChanged)
    Q_PROPERTY(QString language READ language WRITE setLanguage NOTIFY languageChanged)
    Q_PROPERTY(QString theme READ theme WRITE setTheme NOTIFY themeChanged)
    Q_PROPERTY(bool translucent READ translucent WRITE setTranslucent NOTIFY translucentChanged)
    Q_PROPERTY(int timeoutSec READ timeoutSec WRITE setTimeoutSec NOTIFY timeoutChanged)
    Q_PROPERTY(QString defaultSshConfig READ defaultSshConfig CONSTANT)

public:
    explicit Backend(QObject *parent = nullptr);
    ~Backend() override;

    CoreClient *client() { return &m_client; }

    QString appState() const { return m_appState; }
    ServerModel *servers() { return &m_servers; }
    QVariantList groups() const { return m_groups; }
    QVariantList tags() const { return m_tags; }
    QString searchText() const { return m_search; }
    void setSearchText(const QString &s);
    QString groupFilter() const { return m_groupFilter; }
    void setGroupFilter(const QString &g);
    QString tagFilter() const { return m_tagFilter; }
    void setTagFilter(const QString &t);
    QString selectedId() const { return m_selectedId; }
    void setSelectedId(const QString &id);
    QVariantMap selected() const;
    bool loadingList() const { return m_loadingList; }
    bool bulkChecking() const { return m_bulkActive; }
    bool anyChecking() const { return m_bulkActive || m_singleChecks > 0; }
    int checkDone() const { return m_checkDone; }
    int checkTotal() const { return m_checkTotal; }
    QVariantMap stats() const { return m_stats; }
    QVariantMap banner() const { return m_banner; }
    bool backupPending() const { return m_backupPending; }
    QString keyPath() const { return m_keyPath; }
    QString dbPath() const { return m_dbPath; }
    QString coreVersion() const { return m_coreVersion; }
    QString fatalText() const { return m_fatalText; }
    QString fatalCode() const { return m_fatalCode; }
    QVariantMap pendingHostKey() const { return m_pendingHostKey; }
    QString language() const { return m_language; }
    void setLanguage(const QString &l);
    QString theme() const { return m_theme; }
    void setTheme(const QString &t);
    bool translucent() const { return m_translucent; }
    void setTranslucent(bool on);
    int timeoutSec() const { return m_timeoutSec; }
    void setTimeoutSec(int s);
    QString defaultSshConfig() const;

    // --- жизненный цикл
    Q_INVOKABLE void start();
    Q_INVOKABLE void initStorage(bool forceNewKey = false);
    Q_INVOKABLE void reload();
    Q_INVOKABLE void finishFirstRun();
    Q_INVOKABLE void dismissBanner();
    Q_INVOKABLE void resetFilters();

    // --- серверы
    // form: name, host, port, user, authMethod, password, keyPath, keyText, passphrase,
    //       description, groupId, newGroupName, tags(list)
    Q_INVOKABLE void saveConnection(const QVariantMap &form, const QString &id);
    Q_INVOKABLE void deleteConnection(const QString &id);
    Q_INVOKABLE void duplicateConnection(const QString &id);
    Q_INVOKABLE void hostKeyFor(const QString &host, int port);
    Q_INVOKABLE QVariantMap serverById(const QString &id) const;

    // --- группы
    Q_INVOKABLE void createGroup(const QString &name);
    Q_INVOKABLE void renameGroup(const QString &id, const QString &name);
    Q_INVOKABLE void deleteGroup(const QString &id);

    // --- проверка
    Q_INVOKABLE void checkOne(const QString &id);
    Q_INVOKABLE void checkAll();
    Q_INVOKABLE void cancelBulkCheck();
    // decision: "accept" | "reject" | "cancel"
    Q_INVOKABLE void answerHostKey(const QString &decision);

    // --- импорт ~/.ssh/config
    Q_INVOKABLE void importPreview(const QString &path);
    // opts: path, previewId, select(list), groupId, tags(list)
    Q_INVOKABLE void importApply(const QVariantMap &opts);

    // --- ключ-файл
    Q_INVOKABLE void exportKey(const QString &pathOrUrl, bool overwrite = false);

    // Для тестов/скриншотов: показать диалог host key без реальной проверки
    void injectHostKey(const QVariantMap &hk) { m_hostKeyQueue.append(hk); maybeShowHostKey(); }

    // --- вспомогательное для QML
    Q_INVOKABLE QString errorText(const QString &code, const QVariantMap &params, const QString &message = {}) const;
    Q_INVOKABLE QString formatTime(const QString &iso) const;
    Q_INVOKABLE QString toLocalPath(const QString &pathOrUrl) const;
    Q_INVOKABLE QString displayPath(const QString &path) const;
    Q_INVOKABLE QString groupName(const QString &id) const;

signals:
    void appStateChanged();
    void groupsChanged();
    void tagsChanged();
    void filterChanged();
    void selectedChanged();
    void loadingListChanged();
    void checkingChanged();
    void statsChanged();
    void bannerChanged();
    void backupPendingChanged();
    void pathsChanged();
    void pendingHostKeyChanged();
    void languageChanged();
    void themeChanged();
    void translucentChanged();
    void timeoutChanged();
    void reloaded();

    // field — имя поля формы, к которому относится ошибка (может быть пустым)
    void saveFinished(bool ok, const QString &id, const QString &field, const QString &message);
    void deleteFinished(bool ok, const QString &message);
    void duplicateFinished(bool ok, const QString &id, const QString &message);
    void groupActionFinished(bool ok, const QString &message);
    void checkFinished(const QString &id);
    void importPreviewReady(bool ok, const QVariantMap &preview, const QString &message);
    void importApplied(bool ok, const QVariantMap &report, const QString &message);
    void exportFinished(bool ok, const QString &path, const QString &message);
    void hostKeyLoaded(const QString &host, int port, const QVariantMap &key);
    void toast(const QString &text, const QString &kind);

private:
    using Callback = std::function<void(const CoreResponse &)>;
    quint64 call(const QStringList &args, Callback cb, const QJsonObject &input = {}, bool hasInput = false);
    void handleWarnings(const CoreResponse &r);
    void setAppState(const QString &s);
    void setFatal(const CoreResponse &r);
    void showBanner(const CoreResponse &r, bool retryable = true);
    void showBannerText(const QString &text, const QString &code, bool retryable);
    void applyVersion(const QVariantMap &data);
    void refreshBackupState();
    void updateFilter();
    void updateStats();
    void setLoadingList(bool on);
    void buildGroups(const QVariantList &items);
    void loadGroups();
    void loadTags();
    void loadConnections();
    void installTranslator();
    void startCheck(const QStringList &ids, bool bulk);
    void runNextBulkChunk();
    void onCheckEvent(const QVariantMap &event);
    void onCheckJobDone(bool bulk);
    void maybeShowHostKey();
    QJsonObject buildConnectionJson(const QVariantMap &form, const QVariantMap &original, bool edit,
                                    const QString &groupId) const;

    CoreClient m_client;
    ServerModel m_servers;
    QSettings m_settings;
    QTranslator m_translator;
    QHash<quint64, Callback> m_callbacks;

    QString m_appState = QStringLiteral("loading");
    QString m_fatalText, m_fatalCode;
    QVariantList m_groups, m_tags;
    QHash<QString, QStringList> m_groupSubtree; // id -> id + потомки
    QHash<QString, QString> m_groupNames;
    QString m_search, m_groupFilter, m_tagFilter, m_selectedId;
    bool m_loadingList = false;
    bool m_reloadPending = false;
    QVariantMap m_stats, m_banner;
    bool m_backupPending = false;
    QString m_keyPath, m_dbPath, m_coreVersion;

    // проверки
    bool m_bulkActive = false;
    bool m_bulkCancelled = false;
    int m_singleChecks = 0;
    int m_checkDone = 0, m_checkTotal = 0;
    QList<QStringList> m_bulkChunks;
    quint64 m_bulkJob = 0;
    QStringList m_bulkIds;
    QHash<quint64, QStringList> m_jobIds; // job -> id серверов (для сброса «идёт проверка»)
    QList<QVariantMap> m_hostKeyQueue;
    QVariantMap m_pendingHostKey;

    QString m_language, m_theme;
    bool m_translucent = true;
    int m_timeoutSec = 10;
};
