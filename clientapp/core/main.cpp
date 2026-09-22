#include <QApplication>
#include <QDir>
#include <QTextCodec>
#include <csignal>

#include "App.h"
#include "Logger.h"

#if defined(Q_OS_WIN)
    #include <windows.h>
#endif

using namespace rustcam;

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

int main(int argc, char* argv[])
{

    QTextCodec::setCodecForLocale(QTextCodec::codecForName("UTF-8"));

    QApplication app(argc, argv);
    app.setApplicationName("rustcam-client");
    app.setOrganizationName("rustcam");
    app.setApplicationVersion("1.0.0");
    QString appDir = QCoreApplication::applicationDirPath();
    if (!QDir::setCurrent(appDir)) {
        qWarning() << "Failed to set working directory:" << appDir;
    }

    app.setAttribute(Qt::AA_UseHighDpiPixmaps);
    app.setAttribute(Qt::AA_EnableHighDpiScaling);

#if QT_VERSION >= QT_VERSION_CHECK(6, 0, 0)
    app.setHighDpiScaleFactorRoundingPolicy(Qt::HighDpiScaleFactorRoundingPolicy::PassThrough);
#endif

    app.installEventFilter(App::instance());

    setupSignalHandlers();

    if (!App::instance()->initialize()) {
        qCritical() << "Failed to initialize application";
        return 1;
    }

    App::instance()->run();

    int ret = app.exec();

    App::instance()->quit();

    return ret;
}
