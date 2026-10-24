#include <QFontDatabase>
#include <QGuiApplication>
#include <QIcon>
#include <QQmlApplicationEngine>
#include <QQuickStyle>
#include <QtQml/qqmlextensionplugin.h>

Q_IMPORT_QML_PLUGIN(SherpaPlugin)

int main(int argc, char *argv[])
{
    QGuiApplication app(argc, argv);
    QCoreApplication::setOrganizationName(QStringLiteral("Sherpa"));
    QCoreApplication::setApplicationName(QStringLiteral("Sherpa"));
    QCoreApplication::setApplicationVersion(QStringLiteral("0.1.0"));

    // Свои контролы поверх Basic: системная тема ОС не влияет на вид
    QQuickStyle::setStyle(QStringLiteral("Basic"));

    for (const char *f : {"JetBrainsMono-Regular.ttf", "JetBrainsMono-Bold.ttf", "Inter-Regular.otf",
                          "Inter-SemiBold.otf", "Inter-Bold.otf"})
        QFontDatabase::addApplicationFont(QStringLiteral(":/fonts/") + QLatin1String(f));

    QQmlApplicationEngine engine;
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                     []() { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    engine.loadFromModule("Sherpa", "Main");
    return app.exec();
}
