//! Интеграционные тесты CLI без сети: конверт, коды выхода, CRUD, группы,
//! теги, ключ-файл, импорт ssh config.

mod common;

use common::Env;
use serde_json::json;

#[test]
fn version_and_protocol_check() {
    let e = Env::new();
    let d = e.ok(&["version"]);
    assert_eq!(d["protocol"], 1);
    assert_eq!(d["initialized"], false);
    assert!(d["core_version"].is_string());
    let (code, params, exit) = e.err(&["version", "--require-protocol", "2"]);
    assert_eq!((code.as_str(), exit), ("protocol.unsupported", 4));
    assert_eq!(params["max"], 1);
}

#[test]
fn not_initialized_and_bad_requests() {
    let e = Env::new();
    assert_eq!(
        e.err(&["conn", "list"]),
        (
            "store.not_initialized".into(),
            json!({"db": e.path("sherpa.db").display().to_string()}),
            3
        )
    );
    let (code, _, exit) = e.err(&["nope"]);
    assert_eq!((code.as_str(), exit), ("request.unknown_command", 2));
    let (code, _, exit) = e.err(&["conn", "list", "--bogus"]);
    assert_eq!((code.as_str(), exit), ("request.invalid_args", 2));
    let (code, params, exit) = e.err(&["conn", "create", "--password=x"]);
    assert_eq!((code.as_str(), exit), ("request.secret_in_args", 2));
    assert_eq!(params["arg"], "--password");
    let e = Env::initialized();
    let (code, _, exit) = e.err_in(&["conn", "list"], Some("{oops"));
    assert_eq!((code.as_str(), exit), ("request.invalid_json", 2));
    let (code, _, _) = e.err_in(&["conn", "list"], Some("[1]"));
    assert_eq!(code, "request.invalid_json");
}

#[test]
fn init_is_idempotent_and_guards_key() {
    let e = Env::new();
    let d = e.ok(&["init"]);
    assert_eq!(d["created"], json!({"config": true, "db": true, "key_file": true}));
    assert_eq!(d["backup_recommended"], true);
    let fp = d["key_fingerprint"].clone();
    let d = e.ok(&["init"]);
    assert_eq!(d["created"], json!({"config": false, "db": false, "key_file": false}));
    assert_eq!(d["key_fingerprint"], fp);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(e.path("sherpa.key")).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    // Секрет, затем потеря ключ-файла.
    let id = e.create(r#"{"name":"a","host":"h","auth":{"method":"password","password":"pw"}}"#);
    std::fs::rename(e.path("sherpa.key"), e.path("saved.key")).unwrap();
    let (code, _, exit) = e.err(&["init"]);
    assert_eq!((code.as_str(), exit), ("store.key_file_missing", 3));
    let (code, _, _) = e.err(&["conn", "duplicate", &id]);
    assert_eq!(code, "store.key_file_missing");
    // Чужой ключ не подходит.
    let other = Env::new();
    other.ok(&["init"]);
    std::fs::copy(other.path("sherpa.key"), e.path("sherpa.key")).unwrap();
    let (code, _, exit) = e.err(&["conn", "duplicate", &id]);
    assert_eq!((code.as_str(), exit), ("crypto.key_mismatch", 3));
    // Восстановление копии — снова работает.
    std::fs::rename(e.path("saved.key"), e.path("sherpa.key")).unwrap();
    e.ok(&["conn", "duplicate", &id]);
    // --force-new-key стирает секреты.
    std::fs::remove_file(e.path("sherpa.key")).unwrap();
    let d = e.ok(&["init", "--force-new-key"]);
    assert_eq!(d["created"]["key_file"], true);
    let c = e.ok(&["conn", "get", &id]);
    assert_eq!(c["connection"]["auth"]["has_password"], false);
}

#[test]
fn keyfile_export_and_status() {
    let e = Env::initialized();
    let st = e.ok(&["keyfile", "status"]);
    assert_eq!(st["present"], true);
    assert_eq!(st["matches_db"], true);
    assert_eq!(st["permissions_ok"], true);
    assert_eq!(st["last_export_at"], serde_json::Value::Null);
    let to = e.path("backup.key");
    let d = e.ok(&["keyfile", "export", "--to", to.to_str().unwrap()]);
    assert_eq!(d["key_fingerprint"], st["key_fingerprint"]);
    assert!(!d.to_string().contains(
        &std::fs::read_to_string(&to)
            .unwrap()
            .lines()
            .nth(1)
            .unwrap()
            .to_string()
    ));
    let (code, _, _) = e.err(&["keyfile", "export", "--to", to.to_str().unwrap()]);
    assert_eq!(code, "fs.exists");
    e.ok(&["keyfile", "export", "--to", to.to_str().unwrap(), "--overwrite"]);
    assert!(e.ok(&["keyfile", "status"])["last_export_at"].is_string());
    assert_eq!(e.ok(&["init"])["backup_recommended"], false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(e.path("sherpa.key"), std::fs::Permissions::from_mode(0o644)).unwrap();
        let id = e.create(r#"{"name":"x","host":"h","auth":{"method":"password","password":"p"}}"#);
        let (v, code) = e.run(&["conn", "duplicate", &id]);
        assert_eq!(code, 0);
        assert_eq!(v["warnings"][0]["code"], "crypto.key_file_insecure");
        assert_eq!(e.ok(&["keyfile", "status"])["permissions_ok"], false);
    }
}

#[test]
fn connection_crud_flow() {
    let e = Env::initialized();
    // stdin перекрывает флаги, флаги дополняют.
    let d = e.ok_in(
        &[
            "conn",
            "create",
            "--name",
            "flag-name",
            "--host",
            "h",
            "--tag",
            "Prod",
            "--port",
            "2200",
        ],
        Some(r#"{"name":"web-1","auth":{"method":"password","password":"s3cr3t"}}"#),
    );
    let c = &d["connection"];
    assert_eq!(c["name"], "web-1");
    assert_eq!(c["port"], 2200);
    assert_eq!(c["tags"], json!(["prod"]));
    assert_eq!(c["auth"], json!({"method": "password", "has_password": true}));
    assert_eq!(c["status"]["state"], "unknown");
    assert_eq!(c["source"], "manual");
    assert!(!d.to_string().contains("s3cr3t"));
    let id = c["id"].as_str().unwrap().to_string();

    let (code, params, exit) = e.err_in(&["conn", "create"], Some(r#"{"name":"WEB-1","host":"x"}"#));
    assert_eq!((code.as_str(), exit), ("conn.name_taken", 1));
    assert_eq!(params["existing_id"], id.as_str());
    assert_eq!(
        e.err_in(&["conn", "create"], Some(r#"{"name":"n"}"#)).0,
        "validation.required"
    );
    assert_eq!(
        e.err_in(&["conn", "create"], Some(r#"{"name":"n","host":"h","port":0}"#))
            .0,
        "validation.out_of_range"
    );
    assert_eq!(
        e.err_in(&["conn", "create"], Some(r#"{"name":"n","host":"h","extra":1}"#))
            .0,
        "validation.invalid"
    );
    assert_eq!(
        e.err_in(
            &["conn", "create"],
            Some(r#"{"name":"n","host":"h","auth":{"method":"key_text","key_text":"junk"}}"#)
        )
        .0,
        "auth.key_invalid"
    );
    assert_eq!(e.err(&["conn", "get", "nope"]).0, "conn.not_found");

    // Ключ текстом: тип и отпечаток видны, сам ключ — нет.
    let key = sherpa_lib::ssh::keys::generate_ed25519_openssh(None).unwrap();
    let body = json!({"name":"k","host":"h","auth":{"method":"key_text","key_text": key.as_str()}}).to_string();
    let k = e.ok_in(&["conn", "create"], Some(&body));
    assert_eq!(k["connection"]["auth"]["key_type"], "ssh-ed25519");
    assert!(k["connection"]["auth"]["public_fingerprint"]
        .as_str()
        .unwrap()
        .starts_with("SHA256:"));
    assert!(!k.to_string().contains("OPENSSH"));

    // update: merge patch.
    let u = e.ok_in(
        &["conn", "update", &id, "--port", "22"],
        Some(r#"{"description":"d","auth":{"password":null}}"#),
    );
    assert_eq!(u["connection"]["port"], 22);
    assert_eq!(u["connection"]["description"], "d");
    assert_eq!(u["connection"]["auth"]["has_password"], false);
    assert_eq!(u["connection"]["tags"], json!(["prod"]), "tags untouched without --tag");
    let u = e.ok(&["conn", "update", &id, "--tag", "a", "--tag", "b"]);
    assert_eq!(u["connection"]["tags"], json!(["a", "b"]));
    let u = e.ok_in(
        &["conn", "update", &id],
        Some(r#"{"description":null,"user":"deploy"}"#),
    );
    assert_eq!(u["connection"]["description"], serde_json::Value::Null);
    assert_eq!(u["connection"]["user"], "deploy");

    // duplicate
    let d1 = e.ok(&["conn", "duplicate", &id]);
    assert_eq!(d1["connection"]["name"], "web-1 (copy)");
    assert_eq!(d1["connection"]["source"], "duplicate");
    let d2 = e.ok(&["conn", "duplicate", &id, "--name", "web-2"]);
    assert_eq!(d2["connection"]["name"], "web-2");

    // delete
    let r = e.ok(&["conn", "delete", &id, "missing"]);
    assert_eq!(r["deleted"], json!([id]));
    assert_eq!(r["not_found"], json!(["missing"]));
    assert_eq!(e.ok(&["conn", "list"])["total"], 3);
}

#[test]
fn list_filters_groups_tags() {
    let e = Env::initialized();
    let prod = e.ok(&["group", "create", "--name", "Prod"])["group"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let db = e.ok_in(
        &["group", "create"],
        Some(&json!({"name":"DB","parent_id":prod}).to_string()),
    )["group"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(e.err(&["group", "create", "--name", "prod"]).0, "group.name_taken");
    assert_eq!(e.err(&["group", "update", &prod, "--parent-id", &db]).0, "group.cycle");
    assert_eq!(
        e.err(&["group", "create", "--name", "x", "--parent-id", "nope"]).0,
        "group.not_found"
    );

    e.create(&json!({"name":"web-1","host":"10.0.0.1","tags":["prod","web"],"group_id":prod}).to_string());
    e.create(&json!({"name":"db-1","host":"db.local","tags":["prod"],"group_id":db}).to_string());
    e.create(r#"{"name":"misc","host":"10.9.9.9"}"#);

    let total = |args: &[&str]| e.ok(&[&["conn", "list"][..], args].concat())["total"].as_u64().unwrap();
    assert_eq!(total(&[]), 3);
    assert_eq!(total(&["--q", "DB"]), 1);
    assert_eq!(total(&["--host", "10."]), 2);
    assert_eq!(total(&["--tag", "prod", "--tag", "web"]), 1);
    assert_eq!(total(&["--group-id", &prod]), 1);
    assert_eq!(total(&["--group-id", &prod, "--recursive"]), 2);
    assert_eq!(total(&["--no-group"]), 1);
    assert_eq!(total(&["--status", "unknown"]), 3);
    let l = e.ok(&["conn", "list", "--sort", "host", "--desc", "--limit", "1"]);
    assert_eq!(l["total"], 3);
    assert_eq!(l["items"].as_array().unwrap().len(), 1);
    assert_eq!(l["items"][0]["name"], "db-1"); // "db.local" > "10.…"

    let groups = e.ok(&["group", "list"]);
    let g = groups["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["name"] == "Prod")
        .unwrap();
    assert_eq!(g["connection_count"], 1);

    let tags = e.ok(&["tag", "list"]);
    assert_eq!(
        tags["items"],
        json!([{"name":"prod","connection_count":2},{"name":"web","connection_count":1}])
    );
    e.ok(&["tag", "rename", "web", "prod"]);
    assert_eq!(
        e.ok(&["tag", "list"])["items"],
        json!([{"name":"prod","connection_count":2}])
    );
    assert_eq!(e.err(&["tag", "delete", "nope"]).0, "tag.not_found");
    e.ok(&["tag", "delete", "prod"]);
    assert_eq!(e.ok(&["tag", "list"])["items"], json!([]));

    // Группы: удаление с переносом в родителя и вместе с подключениями.
    e.ok(&["group", "delete", &db]);
    assert_eq!(total(&["--group-id", &prod]), 2);
    let r = e.ok(&["group", "delete", &prod, "--mode", "delete_connections"]);
    assert_eq!(r["deleted_connections"], 2);
    assert_eq!(total(&[]), 1);
    let root = e.ok(&["group", "create", "--name", "A"])["group"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let child = e.ok(&["group", "create", "--name", "B", "--parent-id", &root])["group"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let moved = e.ok(&["group", "update", &child, "--root", "--name", "B2"]);
    assert_eq!(moved["group"]["parent_id"], serde_json::Value::Null);
    assert_eq!(moved["group"]["name"], "B2");
}

#[test]
fn status_list_offline() {
    let e = Env::initialized();
    let id = e.create(r#"{"name":"a","host":"127.0.0.1","port":1}"#);
    let s = e.ok(&["status", "list"]);
    assert_eq!(s["items"][0]["state"], "unknown");
    // Проверка закрытого порта: offline, код выхода 0.
    let r = e.ok(&["conn", "check", &id, "--timeout", "3"]);
    assert_eq!(r["items"][0]["state"], "offline");
    assert_eq!(r["items"][0]["error"]["code"], "net.refused");
    assert_eq!(r["summary"]["offline"], 1);
    let s = e.ok(&["status", "list", &id]);
    assert_eq!(s["items"][0]["state"], "offline");
    assert!(s["items"][0]["checked_at"].is_string());
    // Смена адреса сбрасывает статус.
    let u = e.ok(&["conn", "update", &id, "--port", "2"]);
    assert_eq!(u["connection"]["status"]["state"], "unknown");
    assert_eq!(e.err(&["conn", "check"]).0, "request.invalid_args");
    let ids: Vec<String> = (0..101).map(|i| format!("id{i}")).collect();
    let mut args = vec!["conn", "check"];
    args.extend(ids.iter().map(String::as_str));
    let (code, params, exit) = e.err(&args);
    assert_eq!((code.as_str(), exit), ("request.too_many", 2));
    assert_eq!(params["max"], 100);
}

#[test]
fn dns_failure_is_offline() {
    let e = Env::initialized();
    let id = e.create(r#"{"name":"a","host":"no-such-host.invalid"}"#);
    let r = e.ok(&["conn", "check", &id, "--timeout", "5"]);
    assert_eq!(r["items"][0]["state"], "offline");
    assert_eq!(r["items"][0]["error"]["code"], "net.dns_failed");
}

#[test]
fn import_ssh_config() {
    let e = Env::initialized();
    let ssh = e.path(".ssh");
    std::fs::create_dir_all(ssh.join("conf.d")).unwrap();
    std::fs::write(
        ssh.join("config"),
        "Include conf.d/*\nHost web-1 web-2\n  HostName 10.0.0.%h\nHost db-1\n  HostName db.local\n  Port 2222\n  User postgres\n  ProxyJump bastion\nHost web-*\n  IdentityFile ~/.ssh/web_key\nHost *\n  User root\n",
    )
    .unwrap();
    std::fs::write(ssh.join("conf.d/extra"), "Host extra\n  HostName extra.example\n").unwrap();
    e.create(r#"{"name":"db-1","host":"old"}"#);

    let p = e.ok(&["import", "ssh-config", "preview"]);
    let items = p["items"].as_array().unwrap();
    let by = |k: &str| items.iter().find(|i| i["key"] == k).unwrap().clone();
    assert_eq!(by("extra")["action"], "create");
    let w1 = by("web-1");
    assert_eq!(w1["connection"]["host"], "10.0.0.web-1");
    assert_eq!(w1["connection"]["user"], "root");
    assert_eq!(w1["connection"]["auth"]["method"], "key_file");
    assert_eq!(
        w1["connection"]["auth"]["key_path"],
        e.path(".ssh/web_key").display().to_string()
    );
    assert_eq!(w1["defaults_from"], json!(["Host web-*", "Host *"]));
    assert_eq!(by("db-1")["action"], "skip");
    assert_eq!(by("db-1")["reason"], "name_exists");
    assert!(!items.iter().any(|i| i["key"].as_str().unwrap().contains('*')));
    assert_eq!(p["ignored_directives"][0]["directive"], "proxyjump");
    assert_eq!(p["files"].as_array().unwrap().len(), 2);
    let pid = p["preview_id"].as_str().unwrap().to_string();

    let body = json!({"preview_id": pid, "select": ["web-1", "extra", "db-1", "ghost"], "tags": ["imported"],
                      "overrides": {"db-1": {"name": "db-1-new"}}})
    .to_string();
    let r = e.ok_in(&["import", "ssh-config", "apply"], Some(&body));
    let keys = |k: &str| {
        r[k].as_array()
            .unwrap()
            .iter()
            .map(|i| i["key"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(keys("created"), vec!["extra", "web-1", "db-1"]);
    assert_eq!(keys("skipped"), vec!["web-2"]);
    assert_eq!(r["failed"][0]["error"]["code"], "import.unknown_key");
    let l = e.ok(&["conn", "list", "--tag", "imported"]);
    assert_eq!(l["total"], 3);
    let db = e.ok(&["conn", "list", "--q", "db-1-new"]);
    assert_eq!(db["items"][0]["port"], 2222);
    assert_eq!(db["items"][0]["user"], "postgres");

    // Без select — все новые; повторно — всё уже есть.
    let r = e.ok(&["import", "ssh-config", "apply"]);
    assert_eq!(r["created"][0]["key"], "web-2");
    let r = e.ok(&["import", "ssh-config", "apply"]);
    assert_eq!(r["created"], json!([]));
    // Файл изменился после предпросмотра.
    std::fs::write(ssh.join("conf.d/extra"), "Host extra2\n").unwrap();
    let (code, _, _) = e.err(&["import", "ssh-config", "apply", "--preview-id", &pid]);
    assert_eq!(code, "import.source_changed");
    assert_eq!(
        e.err(&["import", "ssh-config", "preview", "--path", "/nonexistent/x"])
            .0,
        "fs.not_found"
    );
}

#[test]
fn logs_have_no_secrets() {
    let e = Env::initialized();
    let err = e.stderr_of(
        &["conn", "create", "--log-level", "debug"],
        Some(r#"{"name":"a","host":"127.0.0.1","port":1,"auth":{"method":"password","password":"TOPSECRET-123"}}"#),
    );
    assert!(!err.contains("TOPSECRET-123"));
    let err = e.stderr_of(
        &["conn", "check", "--all", "--log-level", "trace", "--timeout", "2"],
        None,
    );
    assert!(!err.contains("TOPSECRET-123"));
}
