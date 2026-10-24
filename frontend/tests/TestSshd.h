// Локальный OpenSSH sshd без root-привилегий хозяина (запускается тем же пользователем) на случайном порту.
#pragma once

#include <QDir>
#include <QFile>
#include <QProcess>
#include <QTcpServer>
#include <QTcpSocket>
#include <unistd.h>
#include <QTemporaryDir>
#include <QThread>

class TestSshd {
public:
    QTemporaryDir dir;
    QProcess proc;
    int port = 0;
    QString clientKeyPath, otherKeyPath, user;

    static QString find()
    {
        for (const char *p : {"/usr/sbin/sshd", "/usr/bin/sshd", "/sbin/sshd"})
            if (QFile::exists(p))
                return p;
        return {};
    }
    static bool run(const QString &prog, const QStringList &args)
    {
        QProcess p;
        p.start(prog, args);
        return p.waitForFinished(20000) && p.exitCode() == 0;
    }
    // Ключи генерируются один раз; смена host key — через newHostKey().
    bool start()
    {
        const QString sshd = find();
        if (sshd.isEmpty() || !dir.isValid())
            return false;
        QDir().mkpath("/run/sshd");
        user = qEnvironmentVariable("USER", "root");
        if (user.isEmpty() || getuid() == 0)
            user = "root";
        clientKeyPath = dir.filePath("client_key");
        otherKeyPath = dir.filePath("other_key");
        if (!run("ssh-keygen", {"-q", "-t", "ed25519", "-N", "", "-f", clientKeyPath})
            || !run("ssh-keygen", {"-q", "-t", "ed25519", "-N", "", "-f", otherKeyPath}))
            return false;
        QFile::copy(clientKeyPath + ".pub", dir.filePath("authorized_keys"));
        QTcpServer s;
        if (!s.listen(QHostAddress::LocalHost, 0))
            return false;
        port = s.serverPort();
        s.close();
        return newHostKey() && launch();
    }
    bool newHostKey()
    {
        const QString hk = dir.filePath("host_key");
        QFile::remove(hk);
        QFile::remove(hk + ".pub");
        return run("ssh-keygen", {"-q", "-t", "ed25519", "-N", "", "-f", hk});
    }
    bool launch()
    {
        QFile cfg(dir.filePath("sshd_config"));
        if (!cfg.open(QIODevice::WriteOnly | QIODevice::Truncate))
            return false;
        cfg.write(QStringLiteral("Port %1\nListenAddress 127.0.0.1\nHostKey %2\nAuthorizedKeysFile %3\nPidFile %4\n"
                                 "PasswordAuthentication no\nKbdInteractiveAuthentication no\nPubkeyAuthentication yes\n"
                                 "UsePAM no\nStrictModes no\nPermitRootLogin yes\nLogLevel ERROR\n")
                      .arg(port).arg(dir.filePath("host_key"), dir.filePath("authorized_keys"), dir.filePath("sshd.pid")).toUtf8());
        cfg.close();
        proc.start(find(), {"-D", "-e", "-f", dir.filePath("sshd_config")});
        if (!proc.waitForStarted(5000))
            return false;
        for (int i = 0; i < 50; ++i) {
            QTcpSocket t;
            t.connectToHost("127.0.0.1", port);
            if (t.waitForConnected(200))
                return true;
            QThread::msleep(100);
        }
        return false;
    }
    void stop()
    {
        if (proc.state() != QProcess::NotRunning) {
            proc.kill();
            proc.waitForFinished(3000);
        }
    }
    QString readKey(const QString &path) const
    {
        QFile f(path);
        return f.open(QIODevice::ReadOnly) ? QString::fromUtf8(f.readAll()) : QString();
    }
    ~TestSshd() { stop(); }
};
