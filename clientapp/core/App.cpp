#include "App.h"
#include "Logger.h"
#include "ConfigManager.h"
#include "ThemeManager.h"
#include "PluginManager.h"
#include "MainWindow.h"
#include "LoginWindow.h"
#include "SessionManager.h"
#include "Utils.h"
#include "EventBus.h"
#include "interfaces/IPlugin.h"
#include <QApplication>
#include <QCommandLineParser>
#include <QTimer>
#include <QDir>
#include <QDateTime>
#include <QStyleFactory>
#include <QTranslator>
#include <QLocale>
#include <StyledWidget.h>
#include <ThemeStyle.h>
#ifdef Q_OS_WIN
#include <Windows.h>
#endif

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

#ifdef Q_OS_WIN
    AllocConsole();
    FILE* fp;
    freopen_s(&fp, "CONOUT$", "w", stdout);
#endif

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

    QTranslator* translator = new QTranslator(this);
    if (translator->load("D:/RustCam-Media/clientapp/translations/zh_CN.qm")) {
        QCoreApplication::installTranslator(translator);
        LOG_INFO("App", QString("Translation loaded successfully"));
    } else {
        LOG_ERROR("App", "Failed to load translation");
    }

    ThemeManager::instance()->setTheme(ThemeManager::Theme::MidnightPro);

    QString pluginDir = QCoreApplication::applicationDirPath() + "/rustcam_plugins";
    PluginManager::instance()->loadPlugins(pluginDir);

    LOG_INFO("App", "Application initialized successfully");
    m_initialized = true;

    emit initialized();

    return true;
}

void App::run()
{
    m_loginWindow = new LoginWindow();
    m_loginWindow->setAttribute(Qt::WA_DeleteOnClose);
    connect(m_loginWindow, &LoginWindow::loginSuccess, this, &App::onLoginSuccess);
    m_loginWindow->show();
}

void App::onLoginSuccess(const QJsonObject& userInfo)
{
    Q_UNUSED(userInfo)
    LOG_INFO("App", "User logged in successfully, showing main window");
    m_loginWindow->hide();
    m_loginWindow->deleteLater();
    m_loginWindow = nullptr;
    showMainWindow();

    EventBus::instance()->publish(TOPIC_SESSION_LOGGED_IN);
    LOG_INFO("App", "Published session/loggedIn event");
}

void App::showMainWindow()
{
    ThemeManager::instance()->setTheme(ThemeManager::instance()->currentTheme() == ThemeManager::Theme::None
        ? ThemeManager::Theme::MidnightPro : ThemeManager::instance()->currentTheme());
    MainWindow::instance()->registerPlugins();
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
