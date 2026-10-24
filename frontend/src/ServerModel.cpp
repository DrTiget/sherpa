#include "ServerModel.h"

#include "ErrorText.h"

ServerModel::ServerModel(QObject *parent) : QAbstractListModel(parent) {}

int ServerModel::rowCount(const QModelIndex &parent) const
{
    return parent.isValid() ? 0 : m_view.size();
}

QHash<int, QByteArray> ServerModel::roleNames() const
{
    return {
        {IdRole, "serverId"},       {NameRole, "name"},           {HostRole, "host"},
        {PortRole, "port"},         {UserRole, "user"},           {AuthMethodRole, "authMethod"},
        {GroupIdRole, "groupId"},   {GroupNameRole, "groupName"}, {TagsRole, "tags"},
        {StateRole, "state"},       {CheckedAtRole, "checkedAt"}, {LatencyRole, "latencyMs"},
        {ErrorCodeRole, "errorCode"}, {ErrorTextRole, "errorText"}, {CheckingRole, "checking"},
    };
}

QVariant ServerModel::data(const QModelIndex &index, int role) const
{
    if (!index.isValid() || index.row() >= m_view.size())
        return {};
    const Item &it = m_all.at(m_view.at(index.row()));
    switch (role) {
    case IdRole: return it.raw.value("id");
    case NameRole: return it.raw.value("name");
    case HostRole: return it.raw.value("host");
    case PortRole: return it.raw.value("port");
    case UserRole: return it.raw.value("user");
    case AuthMethodRole: return it.raw.value("auth").toMap().value("method");
    case GroupIdRole: return it.raw.value("group_id");
    case GroupNameRole: return it.groupName;
    case TagsRole: return it.raw.value("tags");
    case StateRole: return it.state;
    case CheckedAtRole: return it.checkedAt;
    case LatencyRole: return it.latencyMs;
    case ErrorCodeRole: return it.errorCode;
    case ErrorTextRole: return it.errorText;
    case CheckingRole: return it.checking;
    }
    return {};
}

void ServerModel::setItems(const QVariantList &connections, const QHash<QString, QString> &groupNames)
{
    // сохраняем текст последней ошибки и «идёт проверка» для тех, кто остался
    QHash<QString, Item> old;
    for (const Item &i : std::as_const(m_all))
        old.insert(i.raw.value("id").toString(), i);

    beginResetModel();
    m_all.clear();
    for (const QVariant &v : connections) {
        Item it;
        it.raw = v.toMap();
        const QString id = it.raw.value("id").toString();
        const QVariantMap st = it.raw.value("status").toMap();
        it.state = st.value("state", "unknown").toString();
        it.checkedAt = st.value("checked_at").toString();
        it.latencyMs = st.contains("latency_ms") && !st.value("latency_ms").isNull()
                           ? st.value("latency_ms").toInt() : -1;
        const QVariantMap err = st.value("error").toMap();
        if (!err.isEmpty()) {
            it.errorCode = err.value("code").toString();
            it.errorText = ErrorText::text(it.errorCode, err.value("params").toMap(),
                                           err.value("message").toString());
        } else if (old.contains(id) && old[id].checkedAt == it.checkedAt) {
            it.errorCode = old[id].errorCode;
            it.errorText = old[id].errorText;
            if (it.latencyMs < 0)
                it.latencyMs = old[id].latencyMs;
        }
        it.checking = old.contains(id) && old[id].checking;
        it.groupName = groupNames.value(it.raw.value("group_id").toString());
        m_all.append(it);
    }
    rebuild();
    endResetModel();
    emit countChanged();
}

void ServerModel::setGroupNames(const QHash<QString, QString> &groupNames)
{
    for (Item &it : m_all)
        it.groupName = groupNames.value(it.raw.value("group_id").toString());
    if (!m_view.isEmpty())
        emit dataChanged(index(0), index(m_view.size() - 1), {GroupNameRole});
}

bool ServerModel::matches(const Item &it) const
{
    if (m_noGroup && !it.raw.value("group_id").toString().isEmpty())
        return false;
    if (!m_groupIds.isEmpty() && !m_groupIds.contains(it.raw.value("group_id").toString()))
        return false;
    const QStringList tags = it.raw.value("tags").toStringList();
    if (!m_tag.isEmpty() && !tags.contains(m_tag))
        return false;
    if (!m_query.isEmpty()) {
        const QString q = m_query.toLower();
        bool hit = it.raw.value("name").toString().toLower().contains(q)
                   || it.raw.value("host").toString().toLower().contains(q)
                   || it.raw.value("user").toString().toLower().contains(q);
        for (const QString &t : tags)
            hit = hit || t.contains(q);
        if (!hit)
            return false;
    }
    return true;
}

void ServerModel::rebuild()
{
    m_view.clear();
    for (int i = 0; i < m_all.size(); ++i)
        if (matches(m_all.at(i)))
            m_view.append(i);
}

void ServerModel::setFilter(const QString &query, const QStringList &groupIds, bool noGroup, const QString &tag)
{
    m_query = query.trimmed();
    m_groupIds = groupIds;
    m_noGroup = noGroup;
    m_tag = tag;
    beginResetModel();
    rebuild();
    endResetModel();
    emit countChanged();
}

int ServerModel::visibleRow(const QString &id) const
{
    for (int r = 0; r < m_view.size(); ++r)
        if (m_all.at(m_view.at(r)).raw.value("id").toString() == id)
            return r;
    return -1;
}

int ServerModel::indexOfVisible(const QString &id) const { return visibleRow(id); }

QVariantMap ServerModel::raw(const QString &id) const
{
    for (const Item &it : m_all)
        if (it.raw.value("id").toString() == id) {
            QVariantMap m = it.raw;
            m.insert("group_name", it.groupName);
            m.insert("state", it.state);
            m.insert("checked_at", it.checkedAt);
            m.insert("latency_ms", it.latencyMs);
            m.insert("error_code", it.errorCode);
            m.insert("error_text", it.errorText);
            m.insert("checking", it.checking);
            return m;
        }
    return {};
}

bool ServerModel::contains(const QString &id) const
{
    for (const Item &it : m_all)
        if (it.raw.value("id").toString() == id)
            return true;
    return false;
}

QStringList ServerModel::allIds() const
{
    QStringList l;
    for (const Item &it : m_all)
        l << it.raw.value("id").toString();
    return l;
}

QStringList ServerModel::visibleIds() const
{
    QStringList l;
    for (int i : m_view)
        l << m_all.at(i).raw.value("id").toString();
    return l;
}

void ServerModel::setChecking(const QString &id, bool on)
{
    for (Item &it : m_all)
        if (it.raw.value("id").toString() == id) {
            it.checking = on;
            const int r = visibleRow(id);
            if (r >= 0)
                emit dataChanged(index(r), index(r), {CheckingRole});
            return;
        }
}

void ServerModel::setAllChecking(const QStringList &ids, bool on)
{
    for (const QString &id : ids)
        setChecking(id, on);
}

void ServerModel::applyCheckResult(const QVariantMap &item, const QString &errorText)
{
    const QString id = item.value("id").toString();
    for (Item &it : m_all) {
        if (it.raw.value("id").toString() != id)
            continue;
        it.state = item.value("state", "unknown").toString();
        it.checkedAt = item.value("checked_at").toString();
        it.latencyMs = item.value("latency_ms").isNull() || !item.contains("latency_ms")
                           ? -1 : item.value("latency_ms").toInt();
        const QVariantMap err = item.value("error").toMap();
        it.errorCode = err.value("code").toString();
        it.errorText = errorText;
        it.checking = false;
        const int r = visibleRow(id);
        if (r >= 0)
            emit dataChanged(index(r), index(r));
        return;
    }
}

void ServerModel::countStates(int &online, int &offline, int &authFailed, int &error, int &unknown) const
{
    online = offline = authFailed = error = unknown = 0;
    for (const Item &it : m_all) {
        if (it.state == QLatin1String("online")) ++online;
        else if (it.state == QLatin1String("offline")) ++offline;
        else if (it.state == QLatin1String("auth_failed")) ++authFailed;
        else if (it.state == QLatin1String("error")) ++error;
        else ++unknown;
    }
}
