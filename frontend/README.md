# Sherpa — десктоп-фронт (Qt 6 / QML)

Фронт MVP «Дом»: первый запуск, список серверов (группы, теги, поиск, статусы), карточка, форма создания/изменения, удаление и дублирование, проверка одного и всех (`conn check --stream`), диалог host key, импорт `~/.ssh/config`, настройки (язык ru/en, тема, таймаут, экспорт ключ-файла). Терминал и рабочий стол — следующий этап: место заложено в `qml/TabHost.qml` (вкладка «Серверы» всегда первая).

Вся бизнес-логика в ядре `sherpa-core` (см. `docs/CORE_API.md`); фронт запускает его через `QProcess`, секреты отдаёт в stdin JSON.

## Устройство

| Путь | Что |
|------|-----|
| `src/CoreClient.*` | асинхронный клиент ядра (QProcess), без зависимости от QML; потоковый режим для `conn check --stream` |
| `src/ErrorText.*` | тексты ошибок по кодам ядра (контекст перевода `Errors`) |
| `src/ServerModel.*` | модель списка с фильтром и точечным обновлением статусов |
| `src/Backend.*` | QML-синглтон: состояние, вызовы ядра, настройки |
| `qml/Theme.qml` | цвета, шрифты, размеры; `S*.qml` — собственные контролы |
| `translations/` | `sherpa_ru.ts`, `sherpa_en.ts` (исходные тексты английские) |
| `tests/` | `tst_coreclient` (против настоящего ядра, временная БД), `tst_screens` (интерфейс offscreen + скриншоты) |
| `docker/` | Dockerfile с Qt 6 и `build.sh` для сборки без установки Qt |

Ядро ищется так: `SHERPA_CORE`, рядом с исполняемым файлом, `PATH`. Хранилище задаётся переменными ядра (`SHERPA_DB`, `SHERPA_KEY_FILE`, `SHERPA_CONFIG`).

## Сборка на Linux

Нужны Qt 6.5+ (Core, Gui, Qml, Quick, QuickControls2, Test, LinguistTools, ShaderTools; QML-модули Qt Quick Controls/Layouts/Dialogs/Effects), CMake 3.21+, Ninja, C++17.

```sh
# Ubuntu/Debian
sudo apt install g++ cmake ninja-build qt6-base-dev qt6-declarative-dev qt6-tools-dev qt6-l10n-tools \
  qt6-shadertools-dev qml6-module-qtquick-controls qml6-module-qtquick-layouts \
  qml6-module-qtquick-dialogs qml6-module-qtquick-window qml6-module-qtquick-templates
(cd ../core && cargo build --release)
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build
SHERPA_CORE=$PWD/../core/target/release/sherpa-core ./build/sherpa
```

Тесты и скриншоты (`screenshots/`):

```sh
export SHERPA_CORE=$PWD/../core/target/release/sherpa-core QT_QPA_PLATFORM=offscreen
ctest --test-dir build --output-on-failure
```

Без установки Qt — в контейнере:

```sh
docker build -t sherpa-qt docker
docker run --rm -v $PWD:/src -v $PWD/../core/target/release:/core:ro sherpa-qt /src/docker/build.sh
```

## Сборка на Windows

1. Установить Qt 6.5+ (MSVC 2022 64-bit или MinGW 64-bit) через Qt Online Installer, CMake и Ninja (входят в установщик Qt).
2. Собрать ядро: `cd core && cargo build --release` → `core\target\release\sherpa-core.exe`.
3. В «Qt 6.x MSVC 2022 64-bit» командной строке:

```bat
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_PREFIX_PATH=C:\Qt\6.8.0\msvc2022_64
cmake --build build
copy ..\core\target\release\sherpa-core.exe build\
windeployqt --qmldir qml build\sherpa.exe
build\sherpa.exe
```

`sherpa-core.exe` кладётся рядом с `sherpa.exe`. Приложение GUI-подсистемы (без консоли).

## Переводы

После изменения строк: `lupdate qml src -ts translations/sherpa_ru.ts translations/sherpa_en.ts`, затем русские тексты — в `scripts/ru_translations.py` и `python3 scripts/ru_translations.py`. Язык переключается в «Настройках» без перезапуска.

## Ограничения этапа

- Терминал не реализован (кнопка «Терминал (скоро)» неактивна, двойной клик по строке запускает проверку).
- Размытие фона панелей — `MultiEffect` (на программном рендере выключено автоматически); размытие окна композитором ОС не подключено.
- Тема «Системная» пока равна тёмной.
