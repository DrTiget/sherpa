#include "CodeHighlighter.h"

#include <QFileInfo>

namespace {
QTextCharFormat fmt(const char *color, bool bold = false, bool italic = false)
{
    QTextCharFormat f;
    f.setForeground(QColor(color));
    if (bold) f.setFontWeight(QFont::Bold);
    if (italic) f.setFontItalic(true);
    return f;
}
QString words(const QStringList &l) { return QStringLiteral("\\b(?:") + l.join('|') + QStringLiteral(")\\b"); }
}

CodeHighlighter::CodeHighlighter(QObject *parent) : QSyntaxHighlighter(parent) {}

void CodeHighlighter::setQuickDocument(QQuickTextDocument *d)
{
    if (m_doc == d)
        return;
    m_doc = d;
    setDocument(d ? d->textDocument() : nullptr);
    emit documentChanged();
}

void CodeHighlighter::setFileName(const QString &f)
{
    m_file = f;
    m_lang = detect(f, m_doc && m_doc->textDocument() ? m_doc->textDocument()->firstBlock().text() : QString());
    rebuild();
    emit fileNameChanged();
}

void CodeHighlighter::setDark(bool d)
{
    if (d == m_dark)
        return;
    m_dark = d;
    rebuild();
    emit darkChanged();
}

QString CodeHighlighter::detect(const QString &fileName, const QString &first)
{
    const QString n = QFileInfo(fileName).fileName().toLower();
    const QString ext = QFileInfo(n).suffix();
    if (first.startsWith("#!")) {
        if (first.contains("python")) return "python";
        if (first.contains("sh") || first.contains("bash")) return "sh";
    }
    if (ext == "sh" || ext == "bash" || n == ".bashrc" || n == ".profile" || n == ".bash_profile" || n == ".zshrc" || n == "bashrc" || n == "profile") return "sh";
    if (ext == "py") return "python";
    if (ext == "json") return "json";
    if (ext == "yml" || ext == "yaml") return "yaml";
    if (ext == "toml" || ext == "ini" || ext == "cfg" || ext == "conf" || ext == "service" || ext == "timer" || ext == "socket" || ext == "desktop") return "ini";
    if (ext == "js" || ext == "ts" || ext == "c" || ext == "h" || ext == "cpp" || ext == "cc" || ext == "hpp" || ext == "go" || ext == "rs" || ext == "java" || ext == "php" || ext == "css") return "c";
    if (ext == "html" || ext == "htm" || ext == "xml" || ext == "svg") return "xml";
    if (n.contains("nginx") || ext == "vhost") return "nginx";
    if (n == "fstab" || n == "hosts" || n == "crontab" || n == "sshd_config" || n == "ssh_config" || n == "sudoers" || n == "passwd" || n == "environment") return "ini";
    return "text";
}

void CodeHighlighter::rebuild()
{
    m_rules.clear();
    m_lineComment.clear();
    m_cBlockComments = false;
    const bool d = m_dark;
    m_comment = fmt(d ? "#6b7488" : "#6a7388", false, true);
    m_string = fmt(d ? "#3dff8a" : "#0e7a3a");
    const QTextCharFormat kw = fmt(d ? "#b388ff" : "#6a3fc0", true);
    const QTextCharFormat num = fmt(d ? "#ffc53d" : "#a86f00");
    const QTextCharFormat var = fmt(d ? "#00e5ff" : "#007c99");
    const QTextCharFormat key = fmt(d ? "#5b9bff" : "#2a5fc0");
    auto add = [&](const QString &re, const QTextCharFormat &f, int group = 0) {
        m_rules.append({QRegularExpression(re), f, group});
    };
    if (m_lang == "sh") {
        m_lineComment = "#";
        add(words({"if","then","else","elif","fi","for","while","until","do","done","case","esac","in","function","return","exit","local","export","source","alias","unset","set","shift","trap","break","continue","select"}), kw);
        add(R"(\$\{?[A-Za-z_][A-Za-z0-9_]*\}?|\$[0-9@#?!*$-])", var);
        add(R"(\b\d+\b)", num);
    } else if (m_lang == "python") {
        m_lineComment = "#";
        add(words({"def","class","import","from","as","if","elif","else","for","while","return","try","except","finally","with","lambda","pass","break","continue","raise","yield","in","is","not","and","or","None","True","False","async","await","global","nonlocal"}), kw);
        add(R"(\b\d+(?:\.\d+)?\b)", num);
    } else if (m_lang == "c") {
        m_lineComment = "//";
        m_cBlockComments = true;
        add(words({"if","else","for","while","do","switch","case","default","return","break","continue","function","const","let","var","class","struct","enum","new","delete","import","export","from","static","void","int","char","long","bool","true","false","null","nullptr","this","public","private","fn","impl","use","mod","pub","func","package","namespace","using","typedef","async","await"}), kw);
        add(R"(\b\d+(?:\.\d+)?\b)", num);
    } else if (m_lang == "json") {
        add(R"("(?:[^"\\]|\\.)*"(?=\s*:))", key);
        add(words({"true","false","null"}), kw);
        add(R"(-?\b\d+(?:\.\d+)?(?:[eE][+-]?\d+)?\b)", num);
    } else if (m_lang == "yaml") {
        m_lineComment = "#";
        add(R"(^\s*-?\s*([A-Za-z0-9_.\-/]+)\s*:)", key, 1);
        add(words({"true","false","null","yes","no"}), kw);
        add(R"(\b\d+(?:\.\d+)?\b)", num);
    } else if (m_lang == "ini") {
        m_lineComment = "#";
        add(R"(^\s*\[[^\]]*\])", kw);
        add(R"(^\s*([A-Za-z0-9_.\-]+)\s*[=\s])", key, 1);
        add(R"(\b\d+\b)", num);
    } else if (m_lang == "nginx") {
        m_lineComment = "#";
        add(R"(^\s*([a-z_]+)\b)", key, 1);
        add(words({"server","location","upstream","http","events","if","map"}), kw);
        add(R"(\$[a-z_]+)", var);
        add(R"(\b\d+[kmgs]?\b)", num);
    } else if (m_lang == "xml") {
        add(R"(</?[A-Za-z][^>\s/]*)", kw);
        add(R"(\s([A-Za-z\-:]+)=)", key, 1);
        add(R"(/?>)", kw);
    }
    rehighlight();
}

void CodeHighlighter::highlightBlock(const QString &text)
{
    for (const Rule &r : std::as_const(m_rules)) {
        auto it = r.re.globalMatch(text);
        while (it.hasNext()) {
            const auto m = it.next();
            setFormat(m.capturedStart(r.group), m.capturedLength(r.group), r.fmt);
        }
    }
    // строки в кавычках
    if (m_lang != "text") {
        static const QRegularExpression strRe(R"("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')");
        auto it = strRe.globalMatch(text);
        while (it.hasNext()) {
            const auto m = it.next();
            if (m_lang == "json" && text.mid(m.capturedEnd()).trimmed().startsWith(':'))
                continue;
            setFormat(m.capturedStart(), m.capturedLength(), m_string);
        }
    }
    if (!m_lineComment.isEmpty()) {
        // комментарий — от маркера вне строки; упрощённо: первый маркер, перед которым чётное число кавычек
        int quotes = 0;
        for (int i = 0; i + m_lineComment.size() <= text.size(); ++i) {
            const QChar c = text.at(i);
            if (c == '"' || c == '\'') ++quotes;
            if (text.mid(i, m_lineComment.size()) == m_lineComment && quotes % 2 == 0 && (m_lang != "sh" || i == 0 || text.at(i - 1) != '$')) {
                setFormat(i, text.size() - i, m_comment);
                break;
            }
        }
    }
    if (m_cBlockComments) {
        setCurrentBlockState(0);
        int start = previousBlockState() == 1 ? 0 : text.indexOf("/*");
        while (start >= 0) {
            const int end = text.indexOf("*/", start);
            int len;
            if (end < 0) { setCurrentBlockState(1); len = text.size() - start; }
            else len = end - start + 2;
            setFormat(start, len, m_comment);
            start = text.indexOf("/*", start + len);
        }
    }
}
