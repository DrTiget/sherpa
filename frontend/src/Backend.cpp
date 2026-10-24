#include "Backend.h"

#include "ErrorText.h"

#include <QCoreApplication>
#include <QJsonArray>
#include <QDateTime>
#include <QDir>
#include <QJsonArray>
#include <QLocale>
#include <QQmlEngine>
#include <QUrl>

namespace {
QJsonArray toJsonArray(const QStringList &l)
{
    QJsonArray a;
    for (const QString &s : l)
        a.append(s);
    return a;
}

QString systemLanguage()
{
    return QLocale::system().name().startsWith(QLatin1String("ru")) ? QStringLiteral("ru")
                                                                     : QStringLiteral("en");
}
} // namespace

Backend::Backend(QObject *parent)
    : QObject(parent),
      m_settings(QSettings::IniFormat, QSettings::UserScope, QStringLiteral("Sherpa"), QStringLiteral("sherpa-ui"))
{
    m_language = m_settings.value("language", systemLanguage()).toString();
    m_theme = m_settings.value("theme", "dark").toString();
    m_translucent = m_settings.value("translucent", true).toBool();
    m_timeoutSec = m_settings.value("timeoutSec", 10).toInt();
    installTranslator();

    // Хранилище/ключ можно переопределить для тестов и портативной установки:
    // sherpa-core сам читает SHERPA_DB, SHERPA_KEY_FILE, SHERPA_CONFIG из окружения.

    connect(&m_client, &CoreClient::finished, this, [this](quint64 id, const CoreResponse &r) {
        const Callback cb = m_callbacks.take(id);
        handleWarnings(r);
        if (cb)
            cb(r);
    });
    connect(&m_client, &CoreClient::streamEvent, this, [this](quint64 id, const QVariantMap &ev) {
        Q_UNUSED(id)
        onCheckEvent(ev);
        if (id == m_bulkJob) {
            ++m_checkDone;
            emit checkingChanged();
        }
    });
}

Backend::~Backend() = default;

quint64 Backend::call(const QStringList &args, Callback cb, const QJsonObject &input, bool hasInput)
{
    const quint64 id = m_client.run(args, input, hasInput);
    m_callbacks.insert(id, std::move(cb));
    return id;
}

void Backend::handleWarnings(const CoreResponse &r)
{
    for (const QVariant &w : r.warnings) {
        const QVariantMap m = w.toMap();
        if (m.value("code").toString() == QLatin1String("crypto.key_file_insecure")) {
            showBannerText(ErrorText::text(m.value("code").toString(), m.value("params").toMap()),
                           m.value("code").toString(), false);
            return;
        }
    }
}

// ---------------------------------------------------------------- состояние

void Backend::setAppState(const QString &s)
{
    if (m_appState == s)
        return;
    m_appState = s;
    emit appStateChanged();
}

void Backend::setFatal(const CoreResponse &r)
{
    m_fatalCode = r.error.code;
    m_fatalText = ErrorText::text(r.error.code, r.error.params, r.error.message);
    m_appState.clear(); // принудительный сигнал
    setAppState(QStringLiteral("fatal"));
}

void Backend::showBannerText(const QString &text, const QString &code, bool retryable)
{
    m_banner = {{"text", text}, {"code", code}, {"retryable", retryable}};
    emit bannerChanged();
}

void Backend::showBanner(const CoreResponse &r, bool retryable)
{
    showBannerText(ErrorText::text(r.error.code, r.error.params, r.error.message), r.error.code, retryable);
}

void Backend::dismissBanner()
{
    if (m_banner.isEmpty())
        return;
    m_banner.clear();
    emit bannerChanged();
}

void Backend::applyVersion(const QVariantMap &data)
{
    m_coreVersion = data.value("core_version").toString();
    const QVariantMap paths = data.value("paths").toMap();
    m_keyPath = paths.value("key_file").toString();
    m_dbPath = paths.value("db").toString();
    emit pathsChanged();
}

void Backend::start()
{
    setAppState(QStringLiteral("loading"));
    call({"version", "--require-protocol", "1"}, [this](const CoreResponse &r) {
        if (!r.ok) {
            setFatal(r);
            return;
        }
        applyVersion(r.data);
        if (!r.data.value("initialized").toBool()) {
            initStorage(false);
            return;
        }
        refreshBackupState();
        reload();
    });
}

void Backend::initStorage(bool forceNewKey)
{
    QStringList args{"init"};
    if (forceNewKey)
        args << "--force-new-key";
    call(args, [this](const CoreResponse &r) {
        if (!r.ok) {
            setFatal(r);
            return;
        }
        const QVariantMap paths = r.data.value("paths").toMap();
        m_keyPath = paths.value("key_file").toString();
        m_dbPath = paths.value("db").toString();
        emit pathsChanged();
        const bool created = r.data.value("created").toMap().value("key_file").toBool();
        m_backupPending = r.data.value("backup_recommended").toBool();
        emit backupPendingChanged();
        if (created || m_backupPending) {
            setAppState(QStringLiteral("firstrun"));
        } else {
            setAppState(QStringLiteral("ready"));
            reload();
        }
    });
}

void Backend::finishFirstRun()
{
    setAppState(QStringLiteral("ready"));
    reload();
}

void Backend::refreshBackupState()
{
    call({"keyfile", "status"}, [this](const CoreResponse &r) {
        if (!r.ok)
            return;
        m_keyPath = r.data.value("path", m_keyPath).toString();
        const bool pending = r.data.value("last_export_at").isNull();
        if (pending != m_backupPending) {
            m_backupPending = pending;
            emit backupPendingChanged();
        }
        emit pathsChanged();
    });
}

// ---------------------------------------------------------------- загрузка списков

void Backend::setLoadingList(bool on)
{
    if (m_loadingList == on)
        return;
    m_loadingList = on;
    emit loadingListChanged();
}

void Backend::reload()
{
    if (m_loadingList) {
        m_reloadPending = true;
        return;
    }
    setLoadingList(true);
    loadGroups();
}

void Backend::loadGroups()
{
    call({"group", "list"}, [this](const CoreResponse &r) {
        if (!r.ok) {
            setLoadingList(false);
            if (m_appState == QLatin1String("ready") && !m_servers.totalCount())
                setFatal(r);
            else
                showBanner(r);
            return;
        }
        buildGroups(r.data.value("items").toList());
        loadTags();
    });
}

void Backend::buildGroups(const QVariantList &items)
{
    QHash<QString, QVariantMap> byId;
    QHash<QString, QStringList> children;
    for (const QVariant &v : items) {
        const QVariantMap g = v.toMap();
        byId.insert(g.value("id").toString(), g);
    }
    QStringList roots;
    for (const QVariant &v : items) {
        const QVariantMap g = v.toMap();
        const QString pid = g.value("parent_id").toString();
        if (pid.isEmpty() || !byId.contains(pid))
            roots << g.value("id").toString();
        else
            children[pid] << g.value("id").toString();
    }
    auto byName = [&](const QString &a, const QString &b) {
        return byId[a].value("name").toString().toLower() < byId[b].value("name").toString().toLower();
    };
    std::sort(roots.begin(), roots.end(), byName);

    m_groups.clear();
    m_groupSubtree.clear();
    QHash<QString, QString> names;
    std::function<QStringList(const QString &, int)> walk = [&](const QString &id, int depth) {
        QVariantMap g = byId[id];
        g.insert("depth", depth);
        names.insert(id, g.value("name").toString());
        const int pos = m_groups.size();
        m_groups.append(g);
        QStringList ids{id};
        QStringList kids = children.value(id);
        std::sort(kids.begin(), kids.end(), byName);
        for (const QString &k : kids)
            ids << walk(k, depth + 1);
        m_groupSubtree.insert(id, ids);
        // счётчик «с подгруппами» для боковой панели
        int total = 0;
        for (const QString &sid : ids)
            total += byId[sid].value("connection_count").toInt();
        QVariantMap upd = m_groups.at(pos).toMap();
        upd.insert("total_count", total);
        m_groups[pos] = upd;
        return ids;
    };
    for (const QString &r : roots)
        walk(r, 0);
    m_groupNames = names;
    m_servers.setGroupNames(names);
    if (!m_groupFilter.isEmpty() && m_groupFilter != QLatin1String("__none__") && !names.contains(m_groupFilter))
        m_groupFilter.clear();
    emit groupsChanged();
}

void Backend::loadTags()
{
    call({"tag", "list"}, [this](const CoreResponse &r) {
        if (r.ok) {
            m_tags = r.data.value("items").toList();
            if (!m_tagFilter.isEmpty()) {
                bool found = false;
                for (const QVariant &t : std::as_const(m_tags))
                    found = found || t.toMap().value("name").toString() == m_tagFilter;
                if (!found)
                    m_tagFilter.clear();
            }
            emit tagsChanged();
        }
        loadConnections();
    });
}

void Backend::loadConnections()
{
    call({"conn", "list", "--sort", "name"}, [this](const CoreResponse &r) {
        setLoadingList(false);
        if (!r.ok) {
            if (m_appState == QLatin1String("ready") && !m_servers.totalCount())
                setFatal(r);
            else
                showBanner(r);
        } else {
            if (m_banner.value("retryable").toBool())
                dismissBanner();
            m_servers.setItems(r.data.value("items").toList(), m_groupNames);
            updateFilter();
            updateStats();
            if (!m_selectedId.isEmpty() && !m_servers.contains(m_selectedId)) {
                m_selectedId.clear();
            }
            emit selectedChanged();
            emit reloaded();
        }
        if (m_reloadPending) {
            m_reloadPending = false;
            reload();
        }
    });
}

void Backend::updateStats()
{
    int on, off, af, er, un;
    m_servers.countStates(on, off, af, er, un);
    m_stats = {{"online", on}, {"offline", off}, {"auth_failed", af}, {"error", er}, {"unknown", un},
               {"total", m_servers.totalCount()}};
    emit statsChanged();
}

// ---------------------------------------------------------------- фильтры и выбор

void Backend::updateFilter()
{
    QStringList ids;
    bool none = false;
    if (m_groupFilter == QLatin1String("__none__"))
        none = true;
    else if (!m_groupFilter.isEmpty())
        ids = m_groupSubtree.value(m_groupFilter, QStringList{m_groupFilter});
    m_servers.setFilter(m_search, ids, none, m_tagFilter);
}

void Backend::setSearchText(const QString &s)
{
    if (m_search == s)
        return;
    m_search = s;
    updateFilter();
    emit filterChanged();
}

void Backend::setGroupFilter(const QString &g)
{
    if (m_groupFilter == g)
        return;
    m_groupFilter = g;
    updateFilter();
    emit filterChanged();
}

void Backend::setTagFilter(const QString &t)
{
    if (m_tagFilter == t)
        return;
    m_tagFilter = t;
    updateFilter();
    emit filterChanged();
}

void Backend::resetFilters()
{
    m_search.clear();
    m_groupFilter.clear();
    m_tagFilter.clear();
    updateFilter();
    emit filterChanged();
}

void Backend::setSelectedId(const QString &id)
{
    if (m_selectedId == id)
        return;
    m_selectedId = id;
    emit selectedChanged();
}

QVariantMap Backend::selected() const
{
    return m_selectedId.isEmpty() ? QVariantMap() : m_servers.raw(m_selectedId);
}

QVariantList Backend::allServers() const
{
    QVariantList out;
    for (const QString &id : m_servers.allIds()) {
        const QVariantMap r = serverById(id);
        out.append(QVariantMap{{"id", id}, {"name", r.value("name")}, {"group_id", r.value("group_id")}, {"group_name", r.value("group_name")}});
    }
    return out;
}

QVariantMap Backend::serverById(const QString &id) const { return m_servers.raw(id); }

QString Backend::groupName(const QString &id) const { return m_groupNames.value(id); }

// ---------------------------------------------------------------- серверы

QJsonObject Backend::buildConnectionJson(const QVariantMap &f, const QVariantMap &orig, bool edit,
                                         const QString &groupId) const
{
    QJsonObject o;
    auto setIfChanged = [&](const char *key, const QJsonValue &v, const QVariant &origV) {
        if (!edit || QJsonValue::fromVariant(origV) != v)
            o[QLatin1String(key)] = v;
    };
    const QString name = f.value("name").toString().trimmed();
    const QString host = f.value("host").toString().trimmed();
    const QString user = f.value("user").toString().trimmed();
    const QString desc = f.value("description").toString().trimmed();
    setIfChanged("name", name, orig.value("name"));
    setIfChanged("host", host, orig.value("host"));
    setIfChanged("port", f.value("port").toInt(), orig.value("port"));
    if (edit) {
        const QJsonValue uv = user.isEmpty() ? QJsonValue(QJsonValue::Null) : QJsonValue(user);
        if (QJsonValue::fromVariant(orig.value("user")) != uv && !(user.isEmpty() && orig.value("user").toString().isEmpty()))
            o["user"] = uv;
        const QJsonValue dv = desc.isEmpty() ? QJsonValue(QJsonValue::Null) : QJsonValue(desc);
        if (!(desc.isEmpty() && orig.value("description").toString().isEmpty())
            && QJsonValue::fromVariant(orig.value("description")) != dv)
            o["description"] = dv;
        if (groupId != orig.value("group_id").toString())
            o["group_id"] = groupId.isEmpty() ? QJsonValue(QJsonValue::Null) : QJsonValue(groupId);
    } else {
        if (!user.isEmpty())
            o["user"] = user;
        if (!desc.isEmpty())
            o["description"] = desc;
        if (!groupId.isEmpty())
            o["group_id"] = groupId;
    }
    const QStringList tags = f.value("tags").toStringList();
    if (!edit || tags != orig.value("tags").toStringList())
        o["tags"] = toJsonArray(tags);

    // auth
    const QString method = f.value("authMethod", "password").toString();
    const QString origMethod = orig.value("auth").toMap().value("method").toString();
    const QString password = f.value("password").toString();
    const QString keyPath = f.value("keyPath").toString().trimmed();
    const QString keyText = f.value("keyText").toString();
    const QString pass = f.value("passphrase").toString();
    const bool secretGiven = !password.isEmpty() || !keyText.isEmpty() || !pass.isEmpty();
    const bool pathChanged = method == QLatin1String("key_file")
                             && keyPath != orig.value("auth").toMap().value("key_path").toString();
    if (!edit || method != origMethod || secretGiven || pathChanged) {
        QJsonObject a;
        a["method"] = method;
        if (method == QLatin1String("password")) {
            if (!password.isEmpty())
                a["password"] = password;
        } else if (method == QLatin1String("key_file")) {
            a["key_path"] = keyPath;
            if (!pass.isEmpty())
                a["passphrase"] = pass;
        } else if (method == QLatin1String("key_text")) {
            if (!keyText.isEmpty())
                a["key_text"] = keyText;
            if (!pass.isEmpty())
                a["passphrase"] = pass;
        }
        o["auth"] = a;
    }
    return o;
}

void Backend::saveConnection(const QVariantMap &form, const QString &id)
{
    const bool edit = !id.isEmpty();
    const QVariantMap orig = edit ? m_servers.raw(id) : QVariantMap();
    const QString newGroup = form.value("newGroupName").toString().trimmed();

    auto doSave = [this, form, id, edit, orig](const QString &groupId) {
        const QJsonObject json = buildConnectionJson(form, orig, edit, groupId);
        QStringList args = edit ? QStringList{"conn", "update", id} : QStringList{"conn", "create"};
        call(args, [this](const CoreResponse &r) {
            if (!r.ok) {
                emit saveFinished(false, {}, ErrorText::field(r.error.code, r.error.params),
                                  ErrorText::text(r.error.code, r.error.params, r.error.message));
                return;
            }
            const QString newId = r.data.value("connection").toMap().value("id").toString();
            m_selectedId = newId;
            emit selectedChanged();
            reload();
            emit saveFinished(true, newId, {}, {});
        }, json, true);
    };

    if (!newGroup.isEmpty()) {
        call({"group", "create", "--name", newGroup}, [this, doSave](const CoreResponse &r) {
            if (!r.ok) {
                emit saveFinished(false, {}, QStringLiteral("group"),
                                  ErrorText::text(r.error.code, r.error.params, r.error.message));
                return;
            }
            doSave(r.data.value("group").toMap().value("id").toString());
        });
    } else {
        doSave(form.value("groupId").toString());
    }
}

void Backend::deleteConnection(const QString &id)
{
    call({"conn", "delete", id}, [this, id](const CoreResponse &r) {
        if (!r.ok) {
            emit deleteFinished(false, ErrorText::text(r.error.code, r.error.params, r.error.message));
            return;
        }
        if (m_selectedId == id) {
            m_selectedId.clear();
            emit selectedChanged();
        }
        reload();
        emit deleteFinished(true, {});
    });
}

void Backend::duplicateConnection(const QString &id)
{
    const QString name = m_servers.raw(id).value("name").toString();
    call({"conn", "duplicate", id, "--name", tr("%1 (copy)").arg(name)}, [this](const CoreResponse &r) {
        if (!r.ok) {
            emit duplicateFinished(false, {}, ErrorText::text(r.error.code, r.error.params, r.error.message));
            return;
        }
        const QString newId = r.data.value("connection").toMap().value("id").toString();
        m_selectedId = newId;
        reload();
        emit duplicateFinished(true, newId, {});
    });
}

// ---------------------------------------------------------------- группы

void Backend::createGroup(const QString &name)
{
    call({"group", "create", "--name", name.trimmed()}, [this](const CoreResponse &r) {
        if (r.ok)
            reload();
        emit groupActionFinished(r.ok, r.ok ? QString() : ErrorText::text(r.error.code, r.error.params, r.error.message));
    });
}

void Backend::renameGroup(const QString &id, const QString &name)
{
    call({"group", "update", id, "--name", name.trimmed()}, [this](const CoreResponse &r) {
        if (r.ok)
            reload();
        emit groupActionFinished(r.ok, r.ok ? QString() : ErrorText::text(r.error.code, r.error.params, r.error.message));
    });
}

void Backend::deleteGroup(const QString &id)
{
    call({"group", "delete", id, "--mode", "move_to_parent"}, [this, id](const CoreResponse &r) {
        if (r.ok) {
            if (m_groupFilter == id)
                setGroupFilter(QString());
            reload();
        }
        emit groupActionFinished(r.ok, r.ok ? QString() : ErrorText::text(r.error.code, r.error.params, r.error.message));
    });
}

// ---------------------------------------------------------------- проверка

void Backend::checkOne(const QString &id)
{
    if (id.isEmpty() || !m_servers.contains(id))
        return;
    startCheck({id}, false);
}

void Backend::checkAll()
{
    if (m_bulkActive)
        return;
    m_bulkIds = m_servers.allIds();
    if (m_bulkIds.isEmpty())
        return;
    m_bulkActive = true;
    m_bulkCancelled = false;
    m_checkDone = 0;
    m_checkTotal = m_bulkIds.size();
    m_bulkChunks.clear();
    for (int i = 0; i < m_bulkIds.size(); i += 100)
        m_bulkChunks << m_bulkIds.mid(i, 100);
    m_servers.setAllChecking(m_bulkIds, true);
    emit checkingChanged();
    runNextBulkChunk();
}

void Backend::runNextBulkChunk()
{
    if (m_bulkChunks.isEmpty() || m_bulkCancelled) {
        m_bulkActive = false;
        m_bulkJob = 0;
        m_servers.setAllChecking(m_bulkIds, false);
        emit checkingChanged();
        updateStats();
        emit selectedChanged();
        maybeShowHostKey();
        return;
    }
    startCheck(m_bulkChunks.takeFirst(), true);
}

void Backend::startCheck(const QStringList &ids, bool bulk)
{
    QStringList args{"conn", "check"};
    args << ids << "--stream" << "--timeout" << QString::number(m_timeoutSec);
    m_servers.setAllChecking(ids, true);
    if (!bulk) {
        ++m_singleChecks;
        emit checkingChanged();
    }
    const quint64 job = m_client.runStream(args);
    if (bulk)
        m_bulkJob = job;
    m_jobIds.insert(job, ids);
    m_callbacks.insert(job, [this, ids, bulk, job](const CoreResponse &r) {
        m_jobIds.remove(job);
        if (!r.ok && r.error.code != QLatin1String("client.cancelled")) {
            showBanner(r, false);
        }
        if (!bulk) {
            m_servers.setAllChecking(ids, false);
            --m_singleChecks;
            emit checkingChanged();
            updateStats();
            emit selectedChanged();
            for (const QString &id : ids)
                emit checkFinished(id);
            if (!m_bulkActive)
                maybeShowHostKey();
        } else {
            runNextBulkChunk();
        }
    });
}

void Backend::cancelBulkCheck()
{
    if (!m_bulkActive)
        return;
    m_bulkCancelled = true;
    if (m_bulkJob)
        m_client.cancel(m_bulkJob);
}

void Backend::onCheckEvent(const QVariantMap &event)
{
    if (event.value("event").toString() != QLatin1String("result"))
        return;
    const QVariantMap item = event.value("item").toMap();
    const QVariantMap err = item.value("error").toMap();
    const QString code = err.value("code").toString();
    const QString text = err.isEmpty() ? QString()
                                       : ErrorText::text(code, err.value("params").toMap(), err.value("message").toString());
    m_servers.applyCheckResult(item, text);
    const QString id = item.value("id").toString();
    if (id == m_selectedId)
        emit selectedChanged();
    updateStats();

    if (code == QLatin1String("hostkey.unknown") || code == QLatin1String("hostkey.mismatch")) {
        const QVariantMap p = err.value("params").toMap();
        QVariantMap q{{"connId", id},
                      {"name", m_servers.raw(id).value("name")},
                      {"kind", code == QLatin1String("hostkey.mismatch") ? "mismatch" : "unknown"},
                      {"host", p.value("host")},
                      {"port", p.value("port")},
                      {"keyType", p.value("key_type")},
                      {"fingerprint", p.value("fingerprint")},
                      {"knownFingerprint", p.value("known_fingerprint")},
                      {"knownAcceptedAt", p.value("known_accepted_at")}};
        for (const QVariantMap &e : std::as_const(m_hostKeyQueue))
            if (e.value("connId") == q.value("connId") && e.value("fingerprint") == q.value("fingerprint"))
                return;
        m_hostKeyQueue.append(q);
    }
}

void Backend::maybeShowHostKey()
{
    if (!m_pendingHostKey.isEmpty() || m_hostKeyQueue.isEmpty())
        return;
    m_pendingHostKey = m_hostKeyQueue.takeFirst();
    emit pendingHostKeyChanged();
}

void Backend::answerHostKey(const QString &decision)
{
    if (m_pendingHostKey.isEmpty())
        return;
    const QVariantMap hk = m_pendingHostKey;
    m_pendingHostKey.clear();
    emit pendingHostKeyChanged();

    const QStringList base{"--host", hk.value("host").toString(), "--port", hk.value("port").toString()};
    if (decision == QLatin1String("accept")) {
        QStringList args{"hostkey", "accept"};
        args << base << "--key-type" << hk.value("keyType").toString() << "--fingerprint"
             << hk.value("fingerprint").toString();
        if (hk.value("kind").toString() == QLatin1String("mismatch"))
            args << "--replace";
        const QString connId = hk.value("connId").toString();
        call(args, [this, connId](const CoreResponse &r) {
            if (!r.ok) {
                emit toast(ErrorText::text(r.error.code, r.error.params, r.error.message), "error");
                maybeShowHostKey();
                return;
            }
            emit toast(tr("Host key saved"), "ok");
            checkOne(connId);
            maybeShowHostKey();
        });
    } else if (decision == QLatin1String("reject")) {
        QStringList args{"hostkey", "reject"};
        args << base << "--fingerprint" << hk.value("fingerprint").toString();
        call(args, [this](const CoreResponse &r) {
            if (!r.ok)
                emit toast(ErrorText::text(r.error.code, r.error.params, r.error.message), "error");
            maybeShowHostKey();
        });
    } else {
        maybeShowHostKey();
    }
}

// ---------------------------------------------------------------- импорт, ключ-файл

void Backend::importPreview(const QString &path)
{
    QStringList args{"import", "ssh-config", "preview"};
    const QString p = toLocalPath(path);
    if (!p.isEmpty())
        args << "--path" << p;
    call(args, [this](const CoreResponse &r) {
        if (!r.ok) {
            emit importPreviewReady(false, {}, ErrorText::text(r.error.code, r.error.params, r.error.message));
            return;
        }
        QVariantMap d = r.data;
        QVariantList texts;
        for (const QVariant &w : d.value("warnings").toList()) {
            const QVariantMap m = w.toMap();
            texts << ErrorText::text(m.value("code").toString(), m.value("params").toMap(), m.value("message").toString());
        }
        d.insert("warning_texts", texts);
        emit importPreviewReady(true, d, {});
    });
}

void Backend::importApply(const QVariantMap &opts)
{
    QJsonObject in;
    const QString p = toLocalPath(opts.value("path").toString());
    if (!p.isEmpty())
        in["path"] = p;
    in["preview_id"] = opts.value("previewId").toString();
    in["select"] = toJsonArray(opts.value("select").toStringList());
    if (!opts.value("groupId").toString().isEmpty())
        in["group_id"] = opts.value("groupId").toString();
    if (!opts.value("tags").toStringList().isEmpty())
        in["tags"] = toJsonArray(opts.value("tags").toStringList());
    call({"import", "ssh-config", "apply"}, [this](const CoreResponse &r) {
        if (!r.ok) {
            emit importApplied(false, {}, ErrorText::text(r.error.code, r.error.params, r.error.message));
            return;
        }
        QVariantMap d = r.data;
        QVariantList failed;
        for (const QVariant &f : d.value("failed").toList()) {
            QVariantMap m = f.toMap();
            const QVariantMap e = m.value("error").toMap();
            m.insert("text", ErrorText::text(e.value("code").toString(), e.value("params").toMap(), e.value("message").toString()));
            failed << m;
        }
        d.insert("failed", failed);
        reload();
        emit importApplied(true, d, {});
    }, in, true);
}

void Backend::exportKey(const QString &pathOrUrl, bool overwrite)
{
    const QString path = toLocalPath(pathOrUrl);
    QStringList args{"keyfile", "export", "--to", path};
    if (overwrite)
        args << "--overwrite";
    call(args, [this, path](const CoreResponse &r) {
        if (!r.ok) {
            emit exportFinished(false, path, ErrorText::text(r.error.code, r.error.params, r.error.message));
            return;
        }
        m_backupPending = false;
        emit backupPendingChanged();
        emit toast(tr("Key file copy saved"), "ok");
        emit exportFinished(true, r.data.value("path", path).toString(), {});
    });
}

// ---------------------------------------------------------------- настройки

void Backend::installTranslator()
{
    QCoreApplication::removeTranslator(&m_translator);
    if (m_language == QLatin1String("en"))
        return;
    if (m_translator.load(QStringLiteral(":/i18n/sherpa_%1.qm").arg(m_language)))
        QCoreApplication::installTranslator(&m_translator);
}

void Backend::setLanguage(const QString &l)
{
    if (l == m_language)
        return;
    m_language = l;
    m_settings.setValue("language", l);
    installTranslator();
    if (QQmlEngine *e = qmlEngine(this))
        e->retranslate();
    // тексты ошибок строк списка хранятся готовыми — перечитаем с новым языком
    reload();
    emit languageChanged();
}

void Backend::setTheme(const QString &t)
{
    if (t == m_theme)
        return;
    m_theme = t;
    m_settings.setValue("theme", t);
    emit themeChanged();
}

void Backend::setTranslucent(bool on)
{
    if (on == m_translucent)
        return;
    m_translucent = on;
    m_settings.setValue("translucent", on);
    emit translucentChanged();
}

void Backend::setTimeoutSec(int s)
{
    s = qBound(1, s, 300);
    if (s == m_timeoutSec)
        return;
    m_timeoutSec = s;
    m_settings.setValue("timeoutSec", s);
    emit timeoutChanged();
}

// ---------------------------------------------------------------- утилиты для QML

QString Backend::errorText(const QString &code, const QVariantMap &params, const QString &message) const
{
    return ErrorText::text(code, params, message);
}

QString Backend::formatTime(const QString &iso) const
{
    if (iso.isEmpty())
        return {};
    const QDateTime dt = QDateTime::fromString(iso, Qt::ISODate).toLocalTime();
    if (!dt.isValid())
        return iso;
    return dt.toString(QStringLiteral("yyyy-MM-dd HH:mm:ss"));
}

QString Backend::toLocalPath(const QString &pathOrUrl) const
{
    if (pathOrUrl.startsWith(QLatin1String("file:")))
        return QUrl(pathOrUrl).toLocalFile();
    return pathOrUrl;
}

QString Backend::displayPath(const QString &path) const
{
    const QString home = QDir::homePath();
#ifdef Q_OS_WIN
    return QDir::toNativeSeparators(path);
#else
    if (!home.isEmpty() && path.startsWith(home + QLatin1Char('/')))
        return QStringLiteral("~") + path.mid(home.size());
    return path;
#endif
}

QString Backend::defaultSshConfig() const
{
    return QDir::toNativeSeparators(QDir::homePath() + QStringLiteral("/.ssh/config"));
}

void Backend::hostKeyFor(const QString &host, int port)
{
    call({"hostkey", "list", "--host", host, "--port", QString::number(port)}, [this, host, port](const CoreResponse &r) {
        QVariantMap key;
        if (r.ok) {
            const QVariantList l = r.data.value("items").toList();
            if (!l.isEmpty())
                key = l.first().toMap();
            else if (r.data.contains("host_key"))
                key = r.data.value("host_key").toMap();
        }
        emit hostKeyLoaded(host, port, key);
    });
}
