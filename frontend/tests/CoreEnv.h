// Общая подготовка для тестов: временная БД и ключ-файл, путь к sherpa-core.
#pragma once
#include <QDir>
#include <QTemporaryDir>
#include <QtGlobal>

struct CoreEnv {
    QTemporaryDir dir;
    CoreEnv()
    {
        qputenv("SHERPA_DB", dir.filePath("sherpa.db").toUtf8());
        qputenv("SHERPA_KEY_FILE", dir.filePath("sherpa.key").toUtf8());
        qputenv("SHERPA_CONFIG", dir.filePath("core.toml").toUtf8());
        qputenv("HOME", dir.path().toUtf8());
        qputenv("XDG_CONFIG_HOME", dir.filePath("xdg").toUtf8());
    }
};
