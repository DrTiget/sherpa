// Модель списка серверов с фильтром (поиск / группа / тег) и обновлением статусов на месте.
#pragma once

#include <QAbstractListModel>
#include <QStringList>
#include <QVariantMap>
#include <QVector>

class ServerModel : public QAbstractListModel {
    Q_OBJECT
    Q_PROPERTY(int count READ rowCount NOTIFY countChanged)
    Q_PROPERTY(int totalCount READ totalCount NOTIFY countChanged)
public:
    enum Roles {
        IdRole = Qt::UserRole + 1,
        NameRole,
        HostRole,
        PortRole,
        UserRole,
        AuthMethodRole,
        GroupIdRole,
        GroupNameRole,
        TagsRole,
        StateRole,
        CheckedAtRole,
        LatencyRole,
        ErrorCodeRole,
        ErrorTextRole,
        CheckingRole,
    };
    Q_ENUM(Roles)

    struct Item {
        QVariantMap raw; // объект Connection из ядра
        QString groupName;
        QString state = QStringLiteral("unknown");
        QString checkedAt;
        int latencyMs = -1;
        QString errorCode;
        QString errorText;
        bool checking = false;
    };

    explicit ServerModel(QObject *parent = nullptr);

    int rowCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    int totalCount() const { return m_all.size(); }
    // Полная замена данных (после conn list). groupNames: id -> имя.
    void setItems(const QVariantList &connections, const QHash<QString, QString> &groupNames);
    void setGroupNames(const QHash<QString, QString> &groupNames);
    // Фильтр: query по имени/хосту/тегам; groupIds — допустимые группы (пусто = все), noGroup — только без группы.
    void setFilter(const QString &query, const QStringList &groupIds, bool noGroup, const QString &tag);

    QVariantMap raw(const QString &id) const;
    bool contains(const QString &id) const;
    QStringList allIds() const;
    QStringList visibleIds() const;
    Q_INVOKABLE int indexOfVisible(const QString &id) const;

    void setChecking(const QString &id, bool on);
    void setAllChecking(const QStringList &ids, bool on);
    // item — элемент ответа conn check (id, state, checked_at, latency_ms, error)
    void applyCheckResult(const QVariantMap &item, const QString &errorText);
    void countStates(int &online, int &offline, int &authFailed, int &error, int &unknown) const;

signals:
    void countChanged();

private:
    void rebuild();
    bool matches(const Item &it) const;
    int visibleRow(const QString &id) const;

    QVector<Item> m_all;
    QVector<int> m_view; // индексы в m_all
    QString m_query;
    QStringList m_groupIds;
    bool m_noGroup = false;
    QString m_tag;
};
