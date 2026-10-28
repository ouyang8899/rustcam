#pragma once

#include <QEventLoop>
#include <QList>
#include <QString>

namespace rustcam {

class MapBrowserClient;

// Owns the process-wide CEF lifetime for the map plugin.
//
// CEF is initialized lazily the first time the plugin needs it and shut down
// (blocking) before the plugin DLL is unloaded. Hot-unloading the plugin is
// not supported: CEF threads and global state cannot be torn down safely
// while the application is running.
class CefRuntime {
public:
    static CefRuntime& instance();

    // Initializes CEF for the browser process. Idempotent.
    bool initialize();

    // Closes all registered browsers and shuts CEF down. Blocking.
    void shutdown();

    bool isInitialized() const { return m_initialized; }

    void addClient(MapBrowserClient* client);
    void removeClient(MapBrowserClient* client);

private:
    CefRuntime() = default;
    ~CefRuntime();

    bool m_initialized = false;
    bool m_shuttingDown = false;
    QList<MapBrowserClient*> m_clients;
    QEventLoop* m_shutdownLoop = nullptr;
};

} // namespace rustcam
