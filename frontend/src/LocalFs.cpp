#include "LocalFs.h"

#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QFileInfo>
#include <QStandardPaths>
#include <QUrl>

QString LocalFs::toPath(const QString &p) const
{
    return p.startsWith(QLatin1String("file:")) ? QUrl(p).toLocalFile() : p;
}

QVariantMap LocalFs::info(const QString &pathOrUrl) const
{
    const QFileInfo fi(toPath(pathOrUrl));
    qint64 size = fi.size();
    if (fi.isDir()) {
        size = 0;
        QDirIterator it(fi.absoluteFilePath(), QDir::Files | QDir::Hidden | QDir::NoSymLinks, QDirIterator::Subdirectories);
        while (it.hasNext()) {
            it.next();
            size += it.fileInfo().size();
        }
    }
    return {{"exists", fi.exists()}, {"isDir", fi.isDir()}, {"size", size}, {"name", fi.fileName()},
            {"path", fi.absoluteFilePath()}};
}

QString LocalFs::home() const { return QDir::homePath(); }

QString LocalFs::downloads() const
{
    const QString d = QStandardPaths::writableLocation(QStandardPaths::DownloadLocation);
    return d.isEmpty() ? QDir::homePath() : d;
}

QString LocalFs::join(const QString &dir, const QString &name) const { return QDir(dir).filePath(name); }
QString LocalFs::baseName(const QString &path) const { return QFileInfo(toPath(path)).fileName(); }
bool LocalFs::exists(const QString &path) const { return QFileInfo::exists(toPath(path)); }

QString LocalFs::uniqueName(const QString &dir, const QString &name) const
{
    const QDir d(toPath(dir));
    if (!d.exists(name))
        return name;
    const QFileInfo fi(name);
    const QString base = fi.completeBaseName(), suf = fi.suffix().isEmpty() ? QString() : "." + fi.suffix();
    for (int i = 1; i < 10000; ++i) {
        const QString n = QStringLiteral("%1 (%2)%3").arg(base).arg(i).arg(suf);
        if (!d.exists(n))
            return n;
    }
    return name;
}

bool LocalFs::writeText(const QString &path, const QString &text) const
{
    QFile f(toPath(path));
    return f.open(QIODevice::WriteOnly | QIODevice::Truncate) && f.write(text.toUtf8()) >= 0;
}

bool LocalFs::remove(const QString &path) const { return QFile::remove(toPath(path)); }
