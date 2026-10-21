# ServerGUI

Десктопная «графическая оболочка для серверов»: приложение ставится на ПК, подключается к серверам по SSH и показывает GUI с модулями. Сейчас реализована **основа**: список серверов, подключение (пароль / ключ / ssh-agent), проверка ключа хоста (TOFU), выполнение команды. Модули (файлы, процессы, службы) — позже.

Стек: Tauri 2, React + TypeScript + Vite, Rust, [russh](https://crates.io/crates/russh) на tokio, [keyring](https://crates.io/crates/keyring).

## Сборка и запуск

Требуется: Rust (rustup), Node.js ≥ 18.

```sh
npm install
npm run tauri dev      # разработка (окно приложения + hot reload)
npm run tauri build    # установщики/бандлы под текущую ОС
npm run build          # только фронтенд (проверка типов + vite build)
cargo test -p servergui-core   # тесты ядра (GUI-зависимости не нужны)
```

Системные зависимости:

- **Linux**: `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev libxdo-dev libdbus-1-dev pkg-config build-essential` (Debian/Ubuntu). Для хранения секретов нужен запущенный Secret Service (gnome-keyring / KWallet).
- **macOS**: Xcode Command Line Tools (`xcode-select --install`). Секреты — в Keychain.
- **Windows**: Microsoft C++ Build Tools и WebView2 (есть в Windows 10/11). Секреты — в Credential Manager.

### Сборка в Docker (Linux, без установки зависимостей)

`docker/Dockerfile.dev` содержит Rust и все библиотеки Tauri; `docker/run.sh` запускает в нём команду:

```sh
docker build -t servergui-dev -f docker/Dockerfile.dev docker
docker/run.sh cargo test --workspace
docker/run.sh cargo build -p servergui
```

### Интеграционный тест (SSH по паролю и ключу)

```sh
scripts/integration.sh
```

Скрипт поднимает `linuxserver/openssh-server` в Docker и проверяет: TOFU, `exec` по паролю, по ключу из файла и по импортированному ключу, неверный пароль, отказ при смене ключа хоста. Без переменных `SSH_TEST_*` тест сам пропускается, так что обычный `cargo test` не зависит от Docker.

## Архитектура

Проект — Cargo workspace из двух крейтов. **`core/`** (`servergui-core`) не зависит от Tauri, поэтому тестируется без GUI-библиотек: `model` (сервер, способ аутентификации; в JSON только *ссылки* на секреты), `store` (CRUD серверов в `servers.json` в `app_config_dir`, запись атомарная), `secrets` (трейт `SecretStore`; в приложении — системное хранилище через keyring, в тестах — память), `known_hosts` (собственный файл known_hosts приложения), `ssh` (пул сессий на russh). **`src-tauri/`** — тонкая обвязка: состояние приложения и Tauri-команды `list/create/update/delete_server`, `test_connection`, `connect`, `disconnect`, `connection_status`, `exec`, `trust_host`.

Соединения живут в `SshPool` (id сервера → сессия), `exec` переиспользует их и открывает новый канал на каждую команду; мёртвая сессия удаляется из пула и пересоздаётся при следующем вызове. Keepalive — каждые 30 с, обрыв после 3 пропусков. Проверка ключа хоста: в `check_server_key` ключ сверяется с known_hosts приложения. Неизвестный хост — соединение прерывается, а фронт получает ошибку `unknown_host` с отпечатком SHA256; после подтверждения пользователем вызывается `trust_host` (сохраняется ровно тот ключ, отпечаток которого показали), и действие повторяется. Несовпадение с сохранённым ключом — отказ с ошибкой `host_key_mismatch`; перезаписать запись автоматически нельзя.

Секреты (пароли, passphrase, вставленные PEM-ключи) уходят из формы сразу в keyring под именами `server:<id>:password|passphrase|key`; при пустом поле при редактировании прежнее значение сохраняется, при удалении сервера — стирается. Ошибки бэкенда сериализуются как `{kind, ...}`, фронт показывает понятный текст.

## Рабочий стол

Интерфейс — «рабочий стол» с настоящими окнами (`src/desktop/`, свой код на pointer events, без библиотек): перетаскивание за заголовок, изменение размера за края и углы, свернуть / развернуть (двойной клик по заголовку) / закрыть, фокус и z-order по клику, минимальный размер, окно не уходит целиком за пределы стола, snap к краям (левая/правая половина, к верхнему краю — на весь стол). Панель задач: «Пуск» (приложения, подменю серверов), окна (клик — фокус/свернуть), выбор активного сервера, индикатор подключений, часы. Раскладка (позиции, размеры, открытые окна, активный сервер) хранится в `localStorage` и восстанавливается при запуске, терминалы переподключаются.

Новое приложение добавляется одной записью в реестр `src/desktop/apps.ts`: `{ id, title, icon, component, defaultSize, needsServer?, singleInstancePerServer? }`. Компонент получает `{ serverId, windowId, setTitle }`. Встроенные: «Серверы», «Терминал» (одно окно — один терминал), «Выполнить команду».

Дымовой UI-тест (Chrome + замоканный Tauri): `scripts/ui-smoke.cjs`.

## Терминал

Команды `terminal_open(server_id, cols, rows, on_event)` / `terminal_write` / `terminal_resize` / `terminal_close`. Терминал — отдельный PTY-канал (`xterm-256color` + shell) поверх соединения из пула; на сервер можно открыть сколько угодно терминалов. Вывод идёт через Tauri `Channel` сырыми байтами (UTF-8 декодирует xterm.js, поэтому границы чанков безопасны), в конце приходит событие `closed` с кодом выхода. `disconnect`/удаление сервера закрывают его терминалы. Фронт: xterm.js + fit/web-links/search; вкладки (двойной клик — переименовать), Ctrl+Shift+C/V — копировать/вставить, Ctrl+Shift+F — поиск, кнопка «Переподключить» после завершения сессии.

### Улучшенная оболочка (в стиле Kali)

Опция на сервер (по умолчанию выкл.; чекбокс в форме сервера или кнопка на панели вкладки терминала). Всё кладётся только в `~/.servergui/` на сервере: `shell/` (zshrc, bashrc, плагины) и файлы истории. `~/.bashrc`, `~/.zshrc` и системные файлы не меняются, root не нужен, интернет на сервере не нужен — файлы вшиты в приложение (`core/assets/shell`) и заливаются одним `tar` через exec-канал.

- Есть zsh → `ZDOTDIR=~/.servergui/shell exec zsh -i`: zsh-autosuggestions (серые подсказки по истории), zsh-syntax-highlighting, меню дополнения (`menu select`), двухстрочный промпт `┌──(user㉿host)-[~/path]` / `└─$`, цветные `ls`/`grep`, алиасы `ll`/`la`.
- Нет zsh → `bash --rcfile ~/.servergui/shell/bashrc -i` с тем же промптом и цветами, bash-completion (если установлен), поиск по истории стрелками; в UI показывается готовая команда установки zsh под дистрибутив (apt/dnf/apk/pacman/zypper).
- Версия набора = хэш файлов; файлы пересылаются заново только при изменении. «Удалить с сервера» стирает `~/.servergui` целиком (включая историю команд).
- Плагины вендорены с лицензиями, версии — в `core/assets/shell/VENDORED.md`. Тема xterm.js по умолчанию — палитра Kali (Tango).

## Ограничения

- Ключи хоста сверяются по алгоритму: при смене алгоритма ключа на сервере хост считается новым.
- Пока нет SFTP, jump-хостов, keyboard-interactive.
- Пользовательский known_hosts не читается (только собственный файл приложения).
