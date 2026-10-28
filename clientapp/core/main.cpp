#include <QApplication>
#include <QDir>
#include <QTextCodec>
#include <QStyleFactory>
#include <QTranslator>
#include <csignal>

#include <curl/curl.h>

#include "include/cef_app.h"
#include "App.h"
#include "Logger.h"
#include "ThemeStyle.h"
#include "bridge_player/include/core/app_logger.h"
#include "bridge_player/include/core/global_context.h"
#include "common/CefBridgeProtocol.h"
#include "common/CefRenderBridge.h"

#if defined(Q_OS_WIN)
    #include <windows.h>
    #include <io.h>
    #include <fcntl.h>
#endif

using rustcam::App;

namespace {

// CefApp used for CEF subprocesses (renderer / gpu / utility).
class RustCamSubprocessApp : public CefApp, public rustcam::cefbridge::RenderProcessBridge {
public:
    CefRefPtr<CefRenderProcessHandler> GetRenderProcessHandler() override
    {
        return this;
    }

private:
    // Include the default reference counting implementation.
    IMPLEMENT_REFCOUNTING(RustCamSubprocessApp);
};

} // namespace

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

    if (GetConsoleWindow() != nullptr) {
        return;
    }

    if (AllocConsole()) {

        FILE* f = nullptr;
        freopen_s(&f, "CONOUT$", "w", stdout);
        freopen_s(&f, "CONOUT$", "w", stderr);
        freopen_s(&f, "CONIN$", "r", stdin);

        std::ios::sync_with_stdio(true);

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
    // CEF subprocess dispatch. This must be the very first thing in main() so
    // that renderer/gpu/utility processes exit before QApplication is
    // constructed and before any plugin is loaded.
    {
        CefMainArgs cefArgs(GetModuleHandle(nullptr));
        CefRefPtr<CefApp> cefApp(new RustCamSubprocessApp());
        const int cefCode = CefExecuteProcess(cefArgs, cefApp.get(), nullptr);
        if (cefCode >= 0) {
            return cefCode;
        }
    }

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
