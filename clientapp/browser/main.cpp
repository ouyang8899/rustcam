#include "BrowserApp.h"
#include "IpcServer.h"
#include "RenderProcessBridge.h"
#include "SessionManager.h"

#include "include/cef_app.h"

#include <QCommandLineParser>
#include <QCoreApplication>
#include <QDir>
#include <QTextStream>
#include <QTimer>

#ifdef Q_OS_WIN
#include <windows.h>
#endif

using rustcam::browser::IpcServer;
using rustcam::browser::SessionManager;

namespace {

/// CEF 渲染/GPU/utility 子进程的 CefApp。
class BrowserSubprocessApp : public CefApp, public rustcam::cefbridge::RenderProcessBridge {
public:
    CefRefPtr<CefRenderProcessHandler> GetRenderProcessHandler() override
    {
        return this;
    }

private:
    IMPLEMENT_REFCOUNTING(BrowserSubprocessApp);
};

} // namespace

int main(int argc, char* argv[])
{
    // CEF 子进程分发：必须是 main() 的第一段，早于 QCoreApplication。
    {
        CefMainArgs cefArgs(GetModuleHandle(nullptr));
        CefRefPtr<CefApp> cefApp(new BrowserSubprocessApp());
        const int cefCode = CefExecuteProcess(cefArgs, cefApp.get(), nullptr);
        if (cefCode >= 0) {
            return cefCode;
        }
    }

    QCoreApplication app(argc, argv);
    app.setApplicationName("rustcam-browser");

    QCommandLineParser parser;
    parser.setApplicationDescription("RustCam embedded browser host");
    const QCommandLineOption pipeOption(QStringList() << "pipe",
        "Logical name of the IPC channel to listen on", "name");
    const QCommandLineOption standaloneOption(QStringList() << "standalone",
        "Open <url> in a standalone top-level window", "url");
    parser.addOption(pipeOption);
    parser.addOption(standaloneOption);
    parser.process(app);

#ifdef Q_OS_WIN
    // QCoreApplication 不会设置 DPI awareness，而 CEF 窗口要作为
    // clientapp 控件的子窗口，必须与之保持一致（spec §5.5）。
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
#endif

    // --- CEF 初始化 ---
    const QString exeDir = QCoreApplication::applicationDirPath();
    QDir().mkpath(exeDir + "/cache");

    CefMainArgs mainArgs(GetModuleHandle(nullptr));
    CefSettings settings;
    settings.no_sandbox = true;
    settings.multi_threaded_message_loop = true;
    settings.windowless_rendering_enabled = false;
    settings.log_severity = LOGSEVERITY_WARNING;
    CefString(&settings.resources_dir_path) = exeDir.toStdString();
    CefString(&settings.locales_dir_path) = (exeDir + "/locales").toStdString();
    CefString(&settings.root_cache_path) = (exeDir + "/cache").toStdString();
    // browser_subprocess_path 留空：CEF 复用本 exe 作为子进程。

    CefRefPtr<CefApp> cefApp(new BrowserApp());
    if (!CefInitialize(mainArgs, settings, cefApp.get(), nullptr)) {
        QTextStream(stderr) << "CefInitialize failed\n";
        return 1;
    }

    SessionManager* manager = new SessionManager(&app);
    manager->startParentWatchdog();
    IpcServer* server = new IpcServer(manager, &app);

    QObject::connect(server, &IpcServer::shutdownRequested, &app, [&]() {
        app.quit();
    });

    const QString pipeName = parser.value(pipeOption);
    if (!pipeName.isEmpty()) {
        QString error;
        if (!server->listen(pipeName, &error)) {
            QTextStream(stderr) << "IPC listen failed: " << error << "\n";
            return 1;
        }
    }

    // standalone 模式：自开顶层窗口加载 URL，不依赖 clientapp。
    //
    // 重要：会话必须在事件循环启动之后创建。CEF 的 CreateBrowser 内部需要
    // 调用线程的消息队列被泵动，否则 OnAfterCreated 永远不触发（spike 实测）。
    // 因此这里用 singleShot(0) 把建会话推到 app.exec() 之后执行。
    const QString standaloneUrl = parser.value(standaloneOption);
    if (!standaloneUrl.isEmpty()) {
        QTimer::singleShot(0, &app, [manager, standaloneUrl]() {
            const QString error = manager->createSession(QStringLiteral("standalone"),
                                                         standaloneUrl, 0,
                                                         QStringLiteral("overlay"));
            if (!error.isEmpty()) {
                QTextStream(stderr) << "standalone session failed: " << error << "\n";
            }
        });
    }

    const int ret = app.exec();

    CefShutdown();
    return ret;
}