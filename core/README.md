# sherpa-core

Ядро Sherpa: CLI-программа, которая хранит SSH-подключения и работает с
серверами по SSH. Фронты (десктоп, позже Laravel) вызывают её и читают JSON из
stdout. Контракт — [`docs/CORE_API.md`](../docs/CORE_API.md), требования —
[`docs/REQUIREMENTS.md`](../docs/REQUIREMENTS.md).

## Структура

```
core/
├── sherpa-core/        бинарник: логи в stderr + sherpa_lib::run_cli
│   └── tests/          интеграционные тесты CLI (assert_cmd), SSH, OpenSSH
└── sherpa-lib/         вся логика
    └── src/
        ├── commands/   реестр групп команд CLI (CommandGroup) и их реализация
        ├── repo/       сущности в БД: conn, group, tag, hostkey
        ├── store/      SQLite + миграции, ключ-файл и шифрование секретов
        ├── ssh/        общий SSH-слой: подключение, TOFU, авторизация, exec
        ├── sshconfig.rs  разбор ~/.ssh/config для импорта
        ├── daemon/     задел демона терминальных сессий (следующий этап)
        ├── config.rs, error.rs, protocol.rs, secret.rs, util.rs
        └── testserver.rs тестовый SSH-сервер (фича test-util)
```

Новая группа команд (например, `monitor`) — модуль в `commands/` с
`impl CommandGroup` и строка в `commands::registry()`. Конверт ответа, коды
ошибок и выхода, stdin-параметры — общие. Удалённые команды выполняются через
`ssh::RemoteExec` поверх `ssh::Session`.

## Сборка и проверки

```sh
cd core
cargo build --release          # target/release/sherpa-core
cargo test                     # юнит + интеграционные (+ OpenSSH sshd, если есть)
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
```

Нужен C-компилятор (bundled SQLite, ring). Windows: таргет
`x86_64-pc-windows-gnu` (mingw-w64) или MSVC.

## Примеры

Ответ всегда один JSON-объект: `{"ok":true,"protocol":1,"data":{…}}` или
`{"ok":false,"protocol":1,"error":{"code","message","params"}}`.
Секреты передаются только через stdin (`--input -`).

```sh
# Версия и проверка совместимости протокола
sherpa-core version --require-protocol 1

# Первый запуск: конфиг, БД, ключ-файл (повторный вызов ничего не меняет)
sherpa-core init
sherpa-core keyfile export --to /media/usb/sherpa.key

# Подключения
echo '{"name":"web-1","host":"10.0.0.5","user":"root",
       "auth":{"method":"password","password":"s3cr3t"}}' \
  | sherpa-core conn create --input - --tag prod
echo '{"auth":{"method":"key_file","key_path":"~/.ssh/id_ed25519","passphrase":"pp"}}' \
  | sherpa-core conn update <id> --input -
sherpa-core conn list --q web --tag prod --sort status
sherpa-core conn duplicate <id>
sherpa-core conn delete <id> <id2>

# Группы и теги
sherpa-core group create --name Prod
sherpa-core group create --name DB --parent-id <group-id>
sherpa-core conn list --group-id <group-id> --recursive
sherpa-core tag rename web frontend

# Проверка: одна, пачкой, потоком
sherpa-core conn check <id>
sherpa-core conn check --all --concurrency 32 --timeout 10
sherpa-core conn check --tag prod --stream     # JSON Lines: строка на сервер + итог

# Host key (TOFU): отпечаток берётся из ошибки hostkey.unknown / hostkey.mismatch
sherpa-core hostkey accept --host 10.0.0.5 --key-type ssh-ed25519 --fingerprint SHA256:…
sherpa-core hostkey accept --host 10.0.0.5 --key-type ssh-ed25519 --fingerprint SHA256:… --replace
sherpa-core hostkey reject --host 10.0.0.5 --fingerprint SHA256:…
sherpa-core hostkey forget --host 10.0.0.5

# Импорт ~/.ssh/config
sherpa-core import ssh-config preview
echo '{"preview_id":"…","select":["web-1","db-1"],"tags":["imported"]}' \
  | sherpa-core import ssh-config apply --input -
```

Глобальные флаги: `--config`, `--db`, `--key-file` (или `SHERPA_CONFIG`,
`SHERPA_DB`, `SHERPA_KEY_FILE`), `--pretty`, `--log-level` (`SHERPA_LOG`).

Коды выхода: 0 — успех, 1 — ошибка, 2 — неверный вызов, 3 — хранилище не
готово, 4 — несовместимая версия, 70 — внутренняя ошибка.
