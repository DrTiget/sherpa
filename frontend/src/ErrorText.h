// Локализованные тексты ошибок ядра по кодам (CORE_API §5). Переводы — в translations/*.ts, контекст "Errors".
#pragma once

#include <QString>
#include <QVariantMap>

namespace ErrorText {
// Текст для UI. Неизвестный код — общий текст + message ядра (CORE_API §1.6).
QString text(const QString &code, const QVariantMap &params, const QString &coreMessage = {});
// Подсказка, к какому полю формы относится ошибка (validation.*, conn.name_taken): "" если ни к какому.
QString field(const QString &code, const QVariantMap &params);
}
