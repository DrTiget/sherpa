# Sherpa — десктоп-фронт (Qt 6 / QML)

Фронт: MVP «Дом» + рабочий стол сервера (терминал, монитор ресурсов). Первый запуск, список серверов (группы, теги, поиск, статусы), карточка, форма создания/изменения, удаление и дублирование, проверка одного и всех (`conn check --stream`), диалог host key, импорт `~/.ssh/config`, настройки (язык ru/en, тема, таймаут, экспорт ключ-файла). Рабочий стол: вкладки серверов (`qml/TabHost.qml`, «Серверы» всегда первая), обои, окна внутри QML (перетаскивание, ресайз, свернуть/развернуть, привязка к краям), панель задач с лаунчером и треем (CPU/RAM/load/uptime/связь, данные `monitor.subscribe`), раскладка окон сохраняется. Окна системных утилит (`qml/*Window.qml`, реестр `qml/SysApps.qml`): «Диски» (разделы, treemap/sunburst, крупные файлы), «Журналы» (journald с фильтрами и follow, файлы `/var/log`), «Планировщик» (cron пользователя, `/etc/cron.d`, таймеры systemd), «Службы», «Пакеты» (потоковый вывод операций), «Система» (hostname, время/NTP, пользователи, группы, reboot/shutdown). Остальные утилиты ядра в лаунчере неактивны с пометкой «скоро».

Вся бизнес-логика в ядре `sherpa-core` (см. `docs/CORE_API.md`); фронт запускает его через `QProcess`, секреты отдаёт в stdin JSON.

## Устройство

| Путь | Что |
|------|-----|
| `src/CoreClient.*` | асинхронный клиент ядра (QProcess), без зависимости от QML; потоковый режим для `conn check --stream` |
| `src/ErrorText.*` | тексты ошибок по кодам ядра (контекст перевода `Errors`) |
| `src/ServerModel.*` | модель списка с фильтром и точечным обновлением статусов |
| `src/Backend.*` | QML-синглтон: состояние, вызовы ядра, настройки |
| `src/DaemonClient.*` | клиент демона: `daemon ensure`, QLocalSocket (Unix socket / named pipe), JSON Lines, hello, автопереподключение |
| `src/DesktopManager.*` | рабочие столы (`desktop.open`, монитор, раскладка `desktop.state.*` или локальный файл) |
| `src/TerminalSession.*`, `src/TermEmulator.*`, `src/TerminalItem.*` | сессия `session.*`, эмулятор VT/xterm и QML-элемент терминала |
| `qml/UtilPane.qml`, `qml/UTable.qml`, `qml/Fmt.qml` | основа окон утилит: запросы через `DesktopState.call` с колбэками, sudo-диалог при `priv.required`, опасные действия (`danger.prepare` → подтверждение с именем сервера → метод с `confirm_token`), таблица, форматирование |
| `qml/Theme.qml` | цвета, шрифты, размеры; `S*.qml` — собственные контролы |
| `translations/` | `sherpa_ru.ts`, `sherpa_en.ts` (исходные тексты английские) |
| `tests/` | `tst_coreclient`, `tst_daemon` (против настоящего ядра, временная БД), `tst_terminal` (эмулятор), `tst_screens` (экраны «Дома» + скриншоты), `tst_sysutils` (окна системных утилит на живом демоне; `SHERPA_EXT_SSH=порт|пользователь|ключ` подключает внешний sshd — например, хоста с systemd), `tst_live` (живой sshd: статусы, host key, рабочий стол, терминал, монитор, обрыв/attach, раскладка) |
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

Тесты: `tst_files` (файлы, редактор, туннели) и `tst_termapps` (вкладки, поиск, vim/htop/tmux, мышь, ресайз) идут на живом sshd; образ с vim/htop/tmux — `docker/Dockerfile`. Тесты и скриншоты (`screenshots/`; `tst_live` пропускается без `sshd`, ему нужен ядро с демоном):

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

- Утилиты рабочего стола, кроме терминала и монитора, пока не подключены (в лаунчере неактивны).
- Раскладка окон: если ядро не знает `desktop.state.get/set`, хранится локально (`AppData/desktops/<id>.json`); путь через ядро написан, но не проверялся на живом ядре.
- В терминале нет разделения панелей, а в рабочем столе — «Обзора рабочих столов».
- Файлы: нет копирования на сервере (в `files.*` нет метода), создания пустого файла, «сохранить как root», архивов; скачивание только через диалог (перетаскивание из окна в ОС не сделано).
- Размытие фона панелей — `MultiEffect` (на программном рендере выключено автоматически); размытие окна композитором ОС не подключено.
- Тема «Системная» пока равна тёмной.

## Реестр утилит и окна рабочего стола

`qml/AppRegistry.qml` — единый реестр: лаунчер, значки и `DesktopPage.openWindow` читают его. Окно подключается записью `{ id, name, glyph, cat, cap, ready: true, source: "ВашеОкно.qml", w, h, single }`; корневой элемент окна принимает `required property var desktop` (`DesktopState`: `rpc(method, params, cb)`, `rpcRaw`, сигнал `eventReceived`, `capabilities`, `readOnlyMode`). Утилита включается в лаунчере, если окно написано (`ready`) и `desktop.capabilities.utilities[cap]` не `false`. Окно может задержать закрытие функцией `beforeClose(proceed)` и открыть другое сигналом `openApp(id, args)`.

### Файлы, редактор, туннели

- `FilesWindow.qml` — SFTP (`files.*`, `transfer.*`): дерево, список (сортировка, выделение, контекстное меню), хлебные крошки и ввод пути, поиск, права (`PermissionsDialog`), создание папок, переименование и перенос, удаление; перетаскивание файлов из ОС на окно — загрузка; панель передач (`FilesTransfers.qml`) с прогрессом, скоростью, ETA, отменой и докачкой, конфликты имён (заменить / оставить оба / докачать / пропустить).
- `EditorWindow.qml` — редактор UTF-8 текста: подсветка (`CodeHighlighter`: sh, python, json, yaml, ini/conf, nginx, c-подобные, xml), номера строк, поиск и замена, сохранение с проверкой `mtime` (при `fs.conflict` — «перезаписать» или «загрузить версию сервера»), подтверждение закрытия с правками.
- `TunnelsWindow.qml` — `tunnel.*`: local/remote/SOCKS, статусы по событиям; нелокальный bind идёт через `danger.prepare` с вводом имени сервера.
- Опасные действия: `DangerDialog` (красный заголовок, перечень, имя сервера) → `danger.prepare` → тот же метод с `confirm_token`.

## Терминал: почему не QTermWidget

QTermWidget — это QWidget и работает только на Unix (pty), а окна рабочего стола — QML-элементы внутри одного окна, причём приложение должно собираться и на Windows. Поэтому эмулятор свой: `TermEmulator` (разбор VT100/xterm: SGR 16/256/truecolor, прокрутка и области, альтернативный экран, мышь SGR, bracketed paste, DEC-графика, широкие символы, ответы DSR/DA) без зависимостей, `TerminalItem` рисует сетку через `QQuickPaintedItem`. Поток данных — `session.*` демона (CORE_API §4.4); вывод копится в эмуляторе, пока окно свёрнуто, при обрыве клиента сессия подхватывается `session.attach`.

## Скриншоты с настоящим GL

Под `offscreen` Qt рисует программно, и размытие `MultiEffect` выключено. Скриншоты с размытием (`screenshots/gl/`) снимаются под Xvfb с Mesa:

```sh
QT_QPA_PLATFORM=xcb SHERPA_SHOTS_DIR=$PWD/screenshots/gl xvfb-run -a -s "-screen 0 1400x900x24" ./build/tests/tst_live
```
