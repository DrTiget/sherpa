// Мелочи для окон утилит: буфер обмена и запись текста в файл.
#pragma once

#include <QObject>
#include <QtQml/qqmlregistration.h>

class ToolUtil : public QObject {
    Q_OBJECT
    QML_ELEMENT
    QML_SINGLETON
public:
    explicit ToolUtil(QObject *parent = nullptr) : QObject(parent) {}
    Q_INVOKABLE void copy(const QString &text);
    // Очищает буфер только если там всё ещё text (пользователь мог скопировать другое)
    Q_INVOKABLE void clearClipboardIf(const QString &text);
    Q_INVOKABLE QString clipboard() const;
    // base64 → текст UTF-8 (потоки логов docker)
    Q_INVOKABLE QString fromBase64(const QString &b64) const;
    Q_INVOKABLE bool writeText(const QString &pathOrUrl, const QString &text);
    Q_INVOKABLE QString defaultExportDir() const;
};
