#include "ToolUtil.h"

#include <QClipboard>
#include <QFile>
#include <QGuiApplication>
#include <QStandardPaths>
#include <QUrl>

void ToolUtil::copy(const QString &text)
{
    if (QClipboard *c = QGuiApplication::clipboard())
        c->setText(text);
}

void ToolUtil::clearClipboardIf(const QString &text)
{
    QClipboard *c = QGuiApplication::clipboard();
    if (c && c->text() == text)
        c->clear();
}

QString ToolUtil::clipboard() const
{
    QClipboard *c = QGuiApplication::clipboard();
    return c ? c->text() : QString();
}

QString ToolUtil::fromBase64(const QString &b64) const
{
    return QString::fromUtf8(QByteArray::fromBase64(b64.toLatin1()));
}

bool ToolUtil::writeText(const QString &pathOrUrl, const QString &text)
{
    const QUrl u(pathOrUrl);
    const QString path = u.isLocalFile() ? u.toLocalFile() : pathOrUrl;
    QFile f(path);
    if (!f.open(QIODevice::WriteOnly | QIODevice::Truncate))
        return false;
    f.write(text.toUtf8());
    return true;
}

QString ToolUtil::defaultExportDir() const
{
    return QStandardPaths::writableLocation(QStandardPaths::DownloadLocation);
}
