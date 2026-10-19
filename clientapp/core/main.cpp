#include <QApplication>
#include <QDir>
#include <QTextCodec>
#include <QStyleFactory>
#include <QTranslator>
#include <csignal>

#include <curl/curl.h>

#include "App.h"
#include "Logger.h"
#include "ThemeStyle.h"
#include "bridge_player/include/core/app_logger.h"
#include "bridge_player/include/core/global_context.h"

#if defined(Q_OS_WIN)
    #include <windows.h>
    #include <io.h>
    #include <fcntl.h>
#endif

using rustcam::App;

void setupSignalHandlers()
{
#if defined(Q_OS_WIN)
    SetConsoleCtrlHandler([](DWORD) -> BOOL {
        App::instance()->quit();
        return TRUE;
    }, TRUE);
#else
    signal(SIGINT, [](int) {
        App::instance()->quit();
    });
    signal(SIGTERM, [](int) {
        App::instance()->quit();
    });
#endif
}
#if defined(Q_OS_WIN)
static void forceConsole() {
    // 已经有控制台就别折腾
    if (GetConsoleWindow() != nullptr) {
        return;
    }

    if (AllocConsole()) {
        // 用 freopen 统一重定向 C 流
        FILE* f = nullptr;
        freopen_s(&f, "CONOUT$", "w", stdout);
        freopen_s(&f, "CONOUT$", "w", stderr);
        freopen_s(&f, "CONIN$", "r", stdin);

        // 让 C++ 流与 C 流同步（默认就是 true，这里显式一下更稳）
        std::ios::sync_with_stdio(true);

        // 关闭缓冲，调试时立刻可见
        setvbuf(stdout, nullptr, _IONBF, 0);
        setvbuf(stderr, nullptr, _IONBF, 0);
        std::cout.setf(std::ios::unitbuf);
        std::cerr.setf(std::ios::unitbuf);

        std::cout << "Console allocated." << std::endl;
    }
}
#endif
int main(int argc, char* argv[])
{
#if defined(Q_OS_WIN)
    forceConsole();
#endif
    QTextCodec::setCodecForLocale(QTextCodec::codecForName("UTF-8"));

    QApplication app(argc, argv);
    app.setApplicationName("rustcam-client");
    app.setOrganizationName("rustcam");
    app.setApplicationVersion("1.0.0");
    app.setStyle(new rustcam::ThemeStyle());
    QString appDir = QCoreApplication::applicationDirPath();
    if (!QDir::setCurrent(appDir)) {
        qWarning() << "Failed to set working directory:" << appDir;
    }

    QTranslator translator;
    QStringList translationPaths = {
        appDir + "/translations/zh_CN.qm",
        appDir + "/../translations/zh_CN.qm",
        ":/translations/zh_CN.qm"
    };
    for (const QString& path : translationPaths) {
        if (translator.load(path)) {
            app.installTranslator(&translator);
            qDebug() << "Loaded translation from:" << path;
            break;
        }
    }

    app.setAttribute(Qt::AA_UseHighDpiPixmaps);
    app.setAttribute(Qt::AA_EnableHighDpiScaling);

#if QT_VERSION >= QT_VERSION_CHECK(6, 0, 0)
    app.setHighDpiScaleFactorRoundingPolicy(Qt::HighDpiScaleFactorRoundingPolicy::PassThrough);
#endif
    std::string log_path = "./logs";
    BridgePlayer::Logger::instance().init(log_path, BridgePlayer::LogLevel::TRACE,
                                        BridgePlayer::Logger::Mode::ASYNC, true);
    APP_LOG_STREAM(BridgePlayer::LogLevel::DEBUG) << "init bridge player logger!";
    curl_global_init(CURL_GLOBAL_DEFAULT);

    BridgePlayer::GlobalContext::instance().install();

    app.installEventFilter(App::instance());

    setupSignalHandlers();

    if (!App::instance()->initialize()) {
        qCritical() << "Failed to initialize application";
        return 1;
    }

    App::instance()->run();

    int ret = app.exec();

    App::instance()->quit();

    BridgePlayer::GlobalContext::instance().uninstall();
    curl_global_cleanup();
    BridgePlayer::Logger::instance().shutdown();

    return ret;
}
