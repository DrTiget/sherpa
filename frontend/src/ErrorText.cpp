#include "ErrorText.h"

#include <QCoreApplication>
#include <QHash>

namespace {
struct Entry {
    const char *code;
    const char *source;
};

// Исходные тексты — английские; ru — в translations/sherpa_ru.ts.
const Entry kEntries[] = {
    {"request.unknown_command", QT_TRANSLATE_NOOP("Errors", "Unknown core command: {command}")},
    {"request.invalid_args", QT_TRANSLATE_NOOP("Errors", "Invalid arguments: {arg} ({reason})")},
    {"request.invalid_json", QT_TRANSLATE_NOOP("Errors", "Core received invalid JSON")},
    {"request.secret_in_args", QT_TRANSLATE_NOOP("Errors", "Secrets must not be passed as arguments")},
    {"request.too_many", QT_TRANSLATE_NOOP("Errors", "Too many items at once: {given} (maximum {max})")},
    {"protocol.unsupported", QT_TRANSLATE_NOOP("Errors", "The core protocol version is not supported (core supports {min}–{max}, the app needs {required}). Update Sherpa.")},
    {"protocol.hello_required", QT_TRANSLATE_NOOP("Errors", "The terminal service expects a handshake")},
    {"protocol.unknown_method", QT_TRANSLATE_NOOP("Errors", "Unknown terminal service method: {method}")},
    {"internal", QT_TRANSLATE_NOOP("Errors", "Internal core error (trace {trace_id}). Details are in the core log.")},

    {"validation.required", QT_TRANSLATE_NOOP("Errors", "This field is required")},
    {"validation.invalid", QT_TRANSLATE_NOOP("Errors", "Invalid value")},
    {"validation.too_long", QT_TRANSLATE_NOOP("Errors", "Too long (maximum {max} characters)")},
    {"validation.out_of_range", QT_TRANSLATE_NOOP("Errors", "Must be between {min} and {max}")},

    {"store.not_initialized", QT_TRANSLATE_NOOP("Errors", "The storage has not been created yet")},
    {"store.key_file_missing", QT_TRANSLATE_NOOP("Errors", "The key file is missing: {path}. Saved secrets cannot be read without it. Restore it from a backup copy.")},
    {"store.permission_denied", QT_TRANSLATE_NOOP("Errors", "No permission to access {path}")},
    {"store.locked", QT_TRANSLATE_NOOP("Errors", "The database is busy: {path}. Try again in a moment.")},
    {"store.corrupted", QT_TRANSLATE_NOOP("Errors", "The database file is damaged: {path}")},
    {"store.schema_too_new", QT_TRANSLATE_NOOP("Errors", "The database was created by a newer version of Sherpa (schema {schema}, supported {supported}). Update the app.")},
    {"store.migration_failed", QT_TRANSLATE_NOOP("Errors", "Database migration failed ({from} → {to})")},
    {"crypto.key_mismatch", QT_TRANSLATE_NOOP("Errors", "The key file does not match the database: {path}")},
    {"crypto.key_file_invalid", QT_TRANSLATE_NOOP("Errors", "The key file is damaged or has the wrong size: {path}")},
    {"crypto.key_file_insecure", QT_TRANSLATE_NOOP("Errors", "The key file permissions are too broad ({mode}): {path}. Restrict access to your user only.")},
    {"crypto.decrypt_failed", QT_TRANSLATE_NOOP("Errors", "Could not decrypt a saved secret ({field})")},
    {"config.invalid", QT_TRANSLATE_NOOP("Errors", "Core config error in {path}, line {line}: {reason}")},
    {"fs.not_found", QT_TRANSLATE_NOOP("Errors", "File not found: {path}")},
    {"fs.exists", QT_TRANSLATE_NOOP("Errors", "The file already exists: {path}")},
    {"fs.read_failed", QT_TRANSLATE_NOOP("Errors", "Could not read {path}: {reason}")},
    {"fs.write_failed", QT_TRANSLATE_NOOP("Errors", "Could not write {path}: {reason}")},

    {"conn.not_found", QT_TRANSLATE_NOOP("Errors", "The server no longer exists (it may have been deleted)")},
    {"conn.name_taken", QT_TRANSLATE_NOOP("Errors", "A server named “{name}” already exists")},
    {"group.not_found", QT_TRANSLATE_NOOP("Errors", "The group was not found")},
    {"group.name_taken", QT_TRANSLATE_NOOP("Errors", "A group named “{name}” already exists")},
    {"group.cycle", QT_TRANSLATE_NOOP("Errors", "A group cannot be moved into its own subgroup")},
    {"tag.not_found", QT_TRANSLATE_NOOP("Errors", "Tag “{name}” was not found")},
    {"tag.invalid", QT_TRANSLATE_NOOP("Errors", "Invalid tag name: “{name}”")},

    {"net.dns_failed", QT_TRANSLATE_NOOP("Errors", "Host name “{host}” cannot be resolved")},
    {"net.refused", QT_TRANSLATE_NOOP("Errors", "Connection refused by {host}:{port} — the SSH port is closed")},
    {"net.unreachable", QT_TRANSLATE_NOOP("Errors", "Host {host} is unreachable")},
    {"net.timeout", QT_TRANSLATE_NOOP("Errors", "Timed out after {timeout_sec} s connecting to {host}:{port}")},
    {"net.reset", QT_TRANSLATE_NOOP("Errors", "The connection to {host} was reset by the remote side")},

    {"ssh.handshake_failed", QT_TRANSLATE_NOOP("Errors", "SSH handshake failed: {reason}")},
    {"ssh.no_common_algorithm", QT_TRANSLATE_NOOP("Errors", "No common SSH algorithms ({kind}); the server offers: {server}")},
    {"ssh.not_ssh", QT_TRANSLATE_NOOP("Errors", "This port is not an SSH server (banner: {banner})")},
    {"ssh.channel_failed", QT_TRANSLATE_NOOP("Errors", "The server refused to open a channel: {reason}")},
    {"ssh.disconnected", QT_TRANSLATE_NOOP("Errors", "The server closed the connection: {reason}")},

    {"auth.rejected", QT_TRANSLATE_NOOP("Errors", "Authentication rejected for {user}: wrong password or the key was not accepted (server allows: {methods})")},
    {"auth.method_not_allowed", QT_TRANSLATE_NOOP("Errors", "The server does not allow “{method}” login (allowed: {methods})")},
    {"auth.secret_required", QT_TRANSLATE_NOOP("Errors", "The password or key passphrase is not saved. Edit the server and enter it.")},
    {"auth.key_invalid", QT_TRANSLATE_NOOP("Errors", "The key is not recognised or its format is unsupported: {reason}")},
    {"auth.passphrase_wrong", QT_TRANSLATE_NOOP("Errors", "Wrong key passphrase")},
    {"auth.key_file_not_found", QT_TRANSLATE_NOOP("Errors", "The key file was not found: {path}")},
    {"auth.key_file_unreadable", QT_TRANSLATE_NOOP("Errors", "Cannot read the key file {path}: {reason}")},
    {"auth.cancelled", QT_TRANSLATE_NOOP("Errors", "Cancelled")},

    {"hostkey.unknown", QT_TRANSLATE_NOOP("Errors", "The server host key is not trusted yet ({fingerprint})")},
    {"hostkey.mismatch", QT_TRANSLATE_NOOP("Errors", "The server host key has changed! This may be a man-in-the-middle attack.")},
    {"hostkey.rejected", QT_TRANSLATE_NOOP("Errors", "This host key was rejected earlier ({fingerprint})")},
    {"hostkey.fingerprint_changed", QT_TRANSLATE_NOOP("Errors", "The server presented a different key than the one you confirmed. Nothing was saved; check again.")},
    {"hostkey.not_found", QT_TRANSLATE_NOOP("Errors", "No saved host key for {host}:{port}")},

    {"import.parse_error", QT_TRANSLATE_NOOP("Errors", "SSH config syntax error in {file}, line {line}: {reason}")},
    {"import.include_cycle", QT_TRANSLATE_NOOP("Errors", "Circular Include in {file}")},
    {"import.include_too_deep", QT_TRANSLATE_NOOP("Errors", "Include nesting is too deep in {file}")},
    {"import.match_unsupported", QT_TRANSLATE_NOOP("Errors", "Match block skipped ({file}, line {line}): not supported")},
    {"import.multiple_identity_files", QT_TRANSLATE_NOOP("Errors", "{key}: several IdentityFile entries, the first one is used")},
    {"import.source_changed", QT_TRANSLATE_NOOP("Errors", "The file changed since the preview: {path}. Build the preview again.")},
    {"import.unknown_key", QT_TRANSLATE_NOOP("Errors", "Host “{key}” is not in the file")},

    {"daemon.start_failed", QT_TRANSLATE_NOOP("Errors", "Could not start the terminal service: {reason}")},
    {"daemon.not_running", QT_TRANSLATE_NOOP("Errors", "The terminal service is not running")},
    {"daemon.already_running", QT_TRANSLATE_NOOP("Errors", "The terminal service is already running (pid {pid})")},
    {"daemon.unreachable", QT_TRANSLATE_NOOP("Errors", "The terminal service does not respond")},
    {"daemon.sessions_active", QT_TRANSLATE_NOOP("Errors", "There are {count} active sessions")},
    {"session.not_found", QT_TRANSLATE_NOOP("Errors", "The session was not found")},
    {"session.not_connected", QT_TRANSLATE_NOOP("Errors", "The session is not connected")},
    {"session.prompt_expired", QT_TRANSLATE_NOOP("Errors", "The prompt is no longer relevant")},
    {"session.limit_reached", QT_TRANSLATE_NOOP("Errors", "Session limit reached ({max})")},

    // Ошибки самого фронта (не от ядра)
    {"client.core_not_found", QT_TRANSLATE_NOOP("Errors", "The sherpa-core program was not found. Reinstall Sherpa or set the SHERPA_CORE variable.")},
    {"client.bad_output", QT_TRANSLATE_NOOP("Errors", "The core returned an unreadable response")},
    {"client.process_failed", QT_TRANSLATE_NOOP("Errors", "The core process crashed")},
    {"client.cancelled", QT_TRANSLATE_NOOP("Errors", "Cancelled")},
};

QString joinValue(const QVariant &v)
{
    if (v.metaType().id() == QMetaType::QVariantList || v.metaType().id() == QMetaType::QStringList)
        return v.toStringList().join(", ");
    return v.toString();
}
} // namespace

namespace ErrorText {

QString text(const QString &code, const QVariantMap &params, const QString &coreMessage)
{
    static const QHash<QString, const char *> table = [] {
        QHash<QString, const char *> h;
        for (const Entry &e : kEntries)
            h.insert(QString::fromLatin1(e.code), e.source);
        return h;
    }();

    const auto it = table.constFind(code);
    if (it == table.constEnd()) {
        const QString generic = QCoreApplication::translate("Errors", "Error %1").arg(code);
        return coreMessage.isEmpty() ? generic : generic + QStringLiteral(": ") + coreMessage;
    }
    QString s = QCoreApplication::translate("Errors", *it);
    for (auto p = params.constBegin(); p != params.constEnd(); ++p)
        s.replace(QStringLiteral("{") + p.key() + QStringLiteral("}"), joinValue(p.value()));
    return s;
}

QString field(const QString &code, const QVariantMap &params)
{
    if (code.startsWith(QLatin1String("validation.")))
        return params.value("field").toString();
    if (code == QLatin1String("conn.name_taken"))
        return QStringLiteral("name");
    return {};
}

} // namespace ErrorText
