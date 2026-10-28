#include "CefRuntime.h"
#include "MapBrowserClient.h"
#include "MapCefApp.h"

#include "common/Logger.h"

#include "include/cef_app.h"
#include "include/cef_base.h"
#include "include/cef_browser.h"

#include <QCoreApplication>
#include <QDir>
#include <QEventLoop>
#include <QTimer>

#include <windows.h>

namespace rustcam {

namespace {

CefRefPtr<CefApp> g_cefApp;

} // namespace

CefRuntime& CefRuntime::instance()
{
    static CefRuntime runtime;
    return runtime;
}

CefRuntime::~CefRuntime()
{
    shutdown();
}

bool CefRuntime::initialize()
{
    if (m_initialized) {
        return true;
    }
    if (m_shuttingDown) {
        return false;
    }

    const QString exeDir = QCoreApplication::applicationDirPath();
    QDir().mkpath(exeDir + "/cache");

    CefMainArgs mainArgs(GetModuleHandle(nullptr));

    CefSettings settings;
    settings.no_sandbox = true;                  // no sandbox by design
    settings.multi_threaded_message_loop = true; // CEF owns the UI thread
    settings.windowless_rendering_enabled = false;
    settings.log_severity = LOGSEVERITY_WARNING;
    CefString(&settings.resources_dir_path) = exeDir.toStdString();
    CefString(&settings.locales_dir_path) = (exeDir + "/locales").toStdString();
    CefString(&settings.root_cache_path) = (exeDir + "/cache").toStdString();
    // browser_subprocess_path intentionally left empty: CEF reuses this exe.

    g_cefApp = new MapCefApp();

    if (!CefInitialize(mainArgs, settings, g_cefApp.get(), nullptr)) {
        LOG_ERROR("CefRuntime", "CefInitialize failed");
        g_cefApp = nullptr;
        return false;
    }

    m_initialized = true;
    LOG_INFO("CefRuntime", QString("CEF initialized, resources=%1").arg(exeDir));
    return true;
}

void CefRuntime::shutdown()
{
    if (!m_initialized || m_shuttingDown) {
        return;
    }
    m_shuttingDown = true;

    if (!m_clients.isEmpty()) {
        for (MapBrowserClient* client : m_clients) {
            if (client) {
                client->closeBrowser();
            }
        }

        // Wait for every browser to report OnBeforeClose() before calling
        // CefShutdown(). QEventLoop::quit() is thread-safe, so the CEF UI
        // thread may signal completion from OnBeforeClose().
        QEventLoop loop;
        m_shutdownLoop = &loop;
        QTimer::singleShot(5000, &loop, &QEventLoop::quit); // timeout guard
        loop.exec();
        m_shutdownLoop = nullptr;
    }

    // CefShutdown() blocks until the CEF UI thread has stopped. It must
    // complete before the plugin DLL is unloaded.
    CefShutdown();
    g_cefApp = nullptr;
    m_clients.clear();
    m_initialized = false;
    m_shuttingDown = false;

    LOG_INFO("CefRuntime", "CEF shut down");
}

void CefRuntime::addClient(MapBrowserClient* client)
{
    if (client && !m_clients.contains(client)) {
        m_clients.append(client);
    }
}

void CefRuntime::removeClient(MapBrowserClient* client)
{
    m_clients.removeAll(client);
    if (m_shutdownLoop && m_clients.isEmpty()) {
        m_shutdownLoop->quit();
    }
}

} // namespace rustcam
