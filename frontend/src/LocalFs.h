// Локальная файловая система для QML: размеры (для прогресса передач), пути, каталоги пользователя.
#pragma once

#include <QObject>
#include <QVariantMap>
#include <QtQml/qqmlregistration.h>

class LocalFs : public QObject {
    Q_OBJECT
    QML_ELEMENT
    QML_SINGLETON
public:
    explicit LocalFs(QObject *parent = nullptr) : QObject(parent) {}
    // {exists, isDir, size (для каталога — сумма файлов), name}
    Q_INVOKABLE QVariantMap info(const QString &pathOrUrl) const;
    Q_INVOKABLE QString toPath(const QString &pathOrUrl) const;
    Q_INVOKABLE QString home() const;
    Q_INVOKABLE QString downloads() const;
    Q_INVOKABLE QString join(const QString &dir, const QString &name) const;
    Q_INVOKABLE QString baseName(const QString &path) const;
    Q_INVOKABLE bool exists(const QString &path) const;
    // Свободное имя в каталоге: name, name (1), name (2)…
    Q_INVOKABLE QString uniqueName(const QString &dir, const QString &name) const;
    Q_INVOKABLE bool writeText(const QString &path, const QString &text) const;
    Q_INVOKABLE bool remove(const QString &path) const;
};
