#include "App.h"
#include "Logger.h"
#include "ConfigManager.h"
#include "ThemeManager.h"
#include "PluginManager.h"
#include "MainWindow.h"
#include "Utils.h"
#include <QApplication>
#include <QCommandLineParser>
#include <QDir>
#include <QDateTime>

namespace rustcam {

App::App(QObject* parent)
    : QObject(parent)
{
}

App::~App()
{
    LOG_INFO("App", "Application shutting down");
    Logger::instance()->flush();
}

App* App::instance()
{
    static App inst;
    return &inst;
}

bool App::initialize()
{
    if (m_initialized) {
        return true;
    }

    setupEnvironment();
    setupLogging();

    LOG_INFO("App", QString("rustcam Client v%1 (build %2)")
                        .arg(version()).arg(buildNumber()));
    LOG_INFO("App", QString("Qt v%1").arg(qVersion()));

    if (!parseArguments()) {
        LOG_ERROR("App", "Failed to parse command line arguments");
        return false;
    }

    ConfigManager::instance();
    ThemeManager::instance();

    QString pluginDir = QCoreApplication::applicationDirPath() + "/plugins";
    PluginManager::instance()->loadPlugins(pluginDir);

    LOG_INFO("App", "Application initialized successfully");
    m_initialized = true;

    emit initialized();

    return true;
}

void App::run()
{
    MainWindow::instance()->show();
}

void App::quit()
{
    emit aboutToQuit();
    qApp->quit();
}

void App::setApplicationName(const QString& name)
{
    QCoreApplication::setApplicationName(name);
}

void App::setOrganizationName(const QString& name)
{
    QCoreApplication::setOrganizationName(name);
}

void App::setupEnvironment()
{
    Utils::ensureDirExists(Utils::getConfigDir());
    Utils::ensureDirExists(Utils::getLogDir());
    Utils::ensureDirExists(Utils::getCacheDir());

    QString logPath = Utils::getLogDir() + "/rustcam-" +
                      QDateTime::currentDateTime().toString("yyyyMMdd") + ".log";
    Logger::instance()->setLogFile(logPath);
    Logger::instance()->setMinLevel(Logger::Info);
}

void App::setupLogging()
{
    LOG_INFO("App", "Logging system initialized");
}

bool App::parseArguments()
{
    QCommandLineParser parser;
    parser.setApplicationDescription("rustcam Video Surveillance Client");
    parser.addHelpOption();
    parser.addVersionOption();

    QCommandLineOption configOption(
        QStringList() << "c" << "config",
        "Specify config file path.",
        "file"
    );
    parser.addOption(configOption);

    QCommandLineOption themeOption(
        QStringList() << "t" << "theme",
        "Set theme (midnight, arctic, obsidian).",
        "theme"
    );
    parser.addOption(themeOption);

    QCommandLineOption fullscreenOption(
        QStringList() << "f" << "fullscreen",
        "Start in fullscreen mode."
    );
    parser.addOption(fullscreenOption);

    parser.process(*qApp);

    if (parser.isSet(configOption)) {
        QString configFile = parser.value(configOption);
        LOG_INFO("App", QString("Using config file: %1").arg(configFile));
    }

    if (parser.isSet(themeOption)) {
        QString theme = parser.value(themeOption).toLower();
        if (theme == "midnight") {
            ThemeManager::instance()->setTheme(ThemeManager::Theme::MidnightPro);
        } else if (theme == "arctic") {
            ThemeManager::instance()->setTheme(ThemeManager::Theme::ArcticClean);
        } else if (theme == "obsidian") {
            ThemeManager::instance()->setTheme(ThemeManager::Theme::ObsidianNeon);
        }
    }

    return true;
}

bool App::eventFilter(QObject* watched, QEvent* event)
{
    if (event->type() == QEvent::FileOpen) {
        return true;
    }
    return QObject::eventFilter(watched, event);
}

}
