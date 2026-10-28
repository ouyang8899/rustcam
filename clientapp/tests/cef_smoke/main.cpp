// Smoke test: verifies that the CEF wrapper links, that libcef.dll loads and
// that CefInitialize() succeeds with the deployed runtime files.
//
// Note: CefShutdown() is intentionally not called here. Shutting CEF down
// inside a short-lived console process currently faults on a CEF background
// thread; the real shutdown path is exercised by the application (see
// CefRuntime::shutdown()).
#include "include/cef_app.h"
#include "include/cef_base.h"
#include "include/cef_browser.h"

#include <QCoreApplication>
#include <QDir>

#include <windows.h>

#include <cstdio>

namespace {

class SmokeApp : public CefApp {
public:
    SmokeApp() = default;

    CefRefPtr<CefBrowserProcessHandler> GetBrowserProcessHandler() override
    {
        return nullptr;
    }

private:
    // Include the default reference counting implementation.
    IMPLEMENT_REFCOUNTING(SmokeApp);
};

} // namespace

#define TRACE(msg) do { std::printf("%s\n", msg); std::fflush(stdout); } while (0)

int main(int argc, char* argv[])
{
    // CEF subprocess dispatch. Must be the first statement in main() so that
    // GPU/utility processes exit before any initialization happens.
    {
        CefMainArgs cefArgs(GetModuleHandle(nullptr));
        CefRefPtr<CefApp> cefApp(new SmokeApp());
        const int cefCode = CefExecuteProcess(cefArgs, cefApp.get(), nullptr);
        if (cefCode >= 0) {
            return cefCode;
        }
    }

    TRACE("step 1: entering main");

    QCoreApplication app(argc, argv);
    TRACE("step 2: QCoreApplication constructed");

    const QString exeDir = QCoreApplication::applicationDirPath();
    std::printf("exeDir=%s\n", exeDir.toStdString().c_str());
    std::fflush(stdout);

    const QString resourcesDir = exeDir;
    const QString localesDir = exeDir + "/locales";
    const QString cacheDir = exeDir + "/cache";
    QDir().mkpath(cacheDir);

    TRACE("step 3: building settings");
    CefMainArgs mainArgs(GetModuleHandle(nullptr));

    CefSettings settings;
    settings.no_sandbox = true;
    settings.multi_threaded_message_loop = true;
    settings.log_severity = LOGSEVERITY_WARNING;
    CefString(&settings.resources_dir_path) = resourcesDir.toStdString();
    CefString(&settings.locales_dir_path) = localesDir.toStdString();
    CefString(&settings.root_cache_path) = cacheDir.toStdString();

    CefRefPtr<CefApp> cefApp(new SmokeApp());
    TRACE("step 4: calling CefInitialize");

    const bool ok = CefInitialize(mainArgs, settings, cefApp.get(), nullptr);
    std::printf("CefInitialize=%d\n", ok ? 1 : 0);
    std::fflush(stdout);
    if (!ok) {
        TRACE("SMOKE FAILED");
        return 1;
    }

    TRACE("SMOKE OK");
    return 0;
}
