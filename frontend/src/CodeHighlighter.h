// Подсветка синтаксиса для редактора файлов: sh, конфиги (ini/conf/yaml/toml), json, python, c-подобные, html/xml, nginx.
#pragma once

#include <QObject>
#include <QQuickTextDocument>
#include <QRegularExpression>
#include <QSyntaxHighlighter>
#include <QTextCharFormat>
#include <QtQml/qqmlregistration.h>

class CodeHighlighter : public QSyntaxHighlighter {
    Q_OBJECT
    QML_ELEMENT
    Q_PROPERTY(QQuickTextDocument *document READ quickDocument WRITE setQuickDocument NOTIFY documentChanged)
    Q_PROPERTY(QString fileName READ fileName WRITE setFileName NOTIFY fileNameChanged)
    Q_PROPERTY(QString language READ language NOTIFY fileNameChanged)
    Q_PROPERTY(bool dark READ dark WRITE setDark NOTIFY darkChanged)
public:
    explicit CodeHighlighter(QObject *parent = nullptr);
    QQuickTextDocument *quickDocument() const { return m_doc; }
    void setQuickDocument(QQuickTextDocument *d);
    QString fileName() const { return m_file; }
    void setFileName(const QString &f);
    QString language() const { return m_lang; }
    bool dark() const { return m_dark; }
    void setDark(bool d);
    static QString detect(const QString &fileName, const QString &firstLine = {});

signals:
    void documentChanged();
    void fileNameChanged();
    void darkChanged();

protected:
    void highlightBlock(const QString &text) override;

private:
    void rebuild();
    struct Rule { QRegularExpression re; QTextCharFormat fmt; int group = 0; };
    QVector<Rule> m_rules;
    QTextCharFormat m_comment, m_string;
    QString m_lineComment;
    bool m_cBlockComments = false;
    QQuickTextDocument *m_doc = nullptr;
    QString m_file, m_lang = "text";
    bool m_dark = true;
};
