// Loads the local map page in CEF and verifies that
//   1. the page and its local assets (maplibre-gl.js/css) load,
//   2. window.rustcamNative is injected by the render process bridge,
//   3. the C++ -> JS direction works (ExecuteJavaScript),
//   4. a JS -> C++ bridge message reaches CefClient::OnProcessMessageReceived.
//
// The browser is embedded as a child window, mirroring the way the map
// plugin hosts it.
//
// Note: the browser process main thread must keep pumping messages, otherwise
// the renderer cannot connect back ("no connection" after 15 seconds). The
// test therefore runs a Qt event loop instead of blocking on a condition
// variable.
#include "include/cef_app.h"
#include "include/cef_browser.h"
#include "include/cef_frame.h"
#include "include/cef_load_handler.h"
#include "include/cef_life_span_handler.h"
#include "include/cef_v8.h"

#include "include/base/cef_bind.h"
#include "include/base/cef_callback.h"
#include "include/wrapper/cef_closure_task.h"

#include "common/CefBridgeProtocol.h"
#include "common/CefRenderBridge.h"

#include <QCoreApplication>
#include <QDir>
#include <QEventLoop>
#include <QFileInfo>
#include <QJsonObject>
#include <QString>
#include <QTimer>

#include <windows.h>

#include <atomic>
#include <cstdio>
#include <string>

namespace {

enum class Step { kLoading, kProbe, kMarker, kDone };

std::atomic<int> g_step{static_cast<int>(Step::kLoading)};
std::string g_bridgePayload;
std::atomic<bool> g_browserCreated{false};
std::atomic<bool> g_probeReceived{false};

class TestApp : public CefApp, public CefBrowserProcessHandler, public rustcam::cefbridge::RenderProcessBridge {
public:
    CefRefPtr<CefBrowserProcessHandler> GetBrowserProcessHandler() override
    {
        return this;
    }

    void OnContextInitialized() override
    {
    }

public:
    void OnBeforeCommandLineProcessing(const CefString& process_type,
                                        CefRefPtr<CefCommandLine> command_line) override
    {
        if (process_type.empty()) {
            command_line->AppendSwitch("in-process-gpu" );
        }
    }

    CefRefPtr<CefRenderProcessHandler> GetRenderProcessHandler() override
    {
        return this;
    }

private:
    IMPLEMENT_REFCOUNTING(TestApp);
};

class TestClient : public CefClient, public CefLoadHandler, public CefLifeSpanHandler {
public:
    CefRefPtr<CefLifeSpanHandler> GetLifeSpanHandler() override { return this; }
    CefRefPtr<CefLoadHandler> GetLoadHandler() override { return this; }

    void OnAfterCreated(CefRefPtr<CefBrowser> browser) override
    {
        m_browser = browser;
        g_browserCreated = true;
    }

    void OnBeforeClose(CefRefPtr<CefBrowser> /*browser*/) override
    {
        m_browser = nullptr;
    }

    void OnLoadEnd(CefRefPtr<CefBrowser> /*browser*/,
                   CefRefPtr<CefFrame> frame,
                   int /*httpStatusCode*/) override
    {
        // Ask the page to report whether the native bridge and the local
        // map library are available.
        const std::string script =
            "sendToNative({"
            "  type: 'probe',"
            "  bridge: (typeof window.rustcamNative === 'function') ? 'function' : 'missing',"
            "  lib: (typeof window.maplibregl !== 'undefined') ? 'loaded' : 'missing',"
            "  mse264: (typeof MediaSource !== 'undefined' && MediaSource.isTypeSupported)"
            "      ? MediaSource.isTypeSupported('video/mp4; codecs=avc1.42E01E') : false,"
            "  mseAac: (typeof MediaSource !== 'undefined' && MediaSource.isTypeSupported)"
            "      ? MediaSource.isTypeSupported('video/mp4; codecs=mp4a.40.2') : false,"
            "  canPlay264: (function(v){ return v.canPlayType('video/mp4; codecs=avc1.42E01E'); })"
            "      (document.createElement('video')),"
            "  flvjs: typeof window.flvjs,"
            "  gl: (function(){"
            "    var c = document.createElement('canvas');"
            "    var gl2 = c.getContext('webgl2');"
            "    var gl1 = c.getContext('webgl');"
            "    var name = gl2 ? 'webgl2' : (gl1 ? 'webgl1' : 'none');"
            "    var renderer = '';"
            "    if (gl2) {"
            "      var dbg = gl2.getExtension('WEBGL_debug_renderer_info');"
            "      renderer = gl2.getParameter(dbg ? dbg.UNMASKED_RENDERER_WEBGL : gl2.RENDERER);"
            "    }"
            "    return name + '/' + renderer;"
            "  })()"
            "});";
        frame->ExecuteJavaScript(script, frame->GetURL().ToString(), 0);
        g_step = static_cast<int>(Step::kProbe);
    }

    void OnLoadError(CefRefPtr<CefBrowser> /*browser*/,
                     CefRefPtr<CefFrame> /*frame*/,
                     ErrorCode /*errorCode*/,
                     const CefString& errorText,
                     const CefString& failedUrl) override
    {
        std::printf("OnLoadError: %s (%s)\n",
                    failedUrl.ToString().c_str(),
                    errorText.ToString().c_str());
        g_step = static_cast<int>(Step::kDone);
    }

    bool OnProcessMessageReceived(CefRefPtr<CefBrowser> /*browser*/,
                                  CefRefPtr<CefFrame> /*frame*/,
                                  CefProcessId /*source_process*/,
                                  CefRefPtr<CefProcessMessage> message) override
    {
        if (!message || message->GetName() != rustcam::cefbridge::kNativeMessageName) {
            return false;
        }
        CefRefPtr<CefListValue> args = message->GetArgumentList();
        if (args && args->GetSize() > 0) {
            g_bridgePayload = args->GetString(0).ToString();
        }
        if (g_bridgePayload.find("markerClicked") != std::string::npos) {
            g_step = static_cast<int>(Step::kMarker);
        }
        if (g_bridgePayload.find("probe") != std::string::npos) {
            g_probeReceived = true;
        }
        return true;
    }

    CefRefPtr<CefBrowser> browser() const { return m_browser; }

private:
    IMPLEMENT_REFCOUNTING(TestClient);

    CefRefPtr<CefBrowser> m_browser;
};

// Hidden native window that hosts the CEF browser, mirroring the way the map
// plugin embeds the browser as a child window.
HWND createHostWindow()
{
    static const wchar_t* kClassName = L"RustCamMapPageTestHost";
    WNDCLASSEXW wc = {};
    wc.cbSize = sizeof(wc);
    wc.lpfnWndProc = DefWindowProcW;
    wc.hInstance = GetModuleHandle(nullptr);
    wc.lpszClassName = kClassName;
    RegisterClassExW(&wc);
    return CreateWindowExW(0, kClassName, L"", WS_OVERLAPPEDWINDOW,
                           0, 0, 800, 600, nullptr, nullptr, wc.hInstance, nullptr);
}

// Resolves the map page next to the deployed plugins.
QString resolveMapPage()
{
    const QString exeDir = QCoreApplication::applicationDirPath();
    const QStringList candidates = {
        exeDir + "/rustcam_plugins/map/web/map.html",
        exeDir + "/../rustcam_plugins/map/web/map.html",
        exeDir + "/map/web/map.html"
    };
    for (const QString& candidate : candidates) {
        if (QFileInfo::exists(candidate)) {
            return QFileInfo(candidate).absoluteFilePath();
        }
    }
    return QString();
}

QString toFileUrl(const QString& path)
{
    QString native = QFileInfo(path).absoluteFilePath();
    native.replace('\\', '/');
    if (!native.startsWith(QLatin1Char('/'))) {
        native.prepend(QLatin1Char('/'));
    }
    return QStringLiteral("file://") + native;
}

} // namespace

int main(int argc, char* argv[])
{
    // CEF subprocess dispatch. Must be the first statement in main() so that
    // GPU/utility processes exit before any initialization happens.
    {
        CefMainArgs cefArgs(GetModuleHandle(nullptr));
        CefRefPtr<CefApp> cefApp(new TestApp());
        const int cefCode = CefExecuteProcess(cefArgs, cefApp.get(), nullptr);
        if (cefCode >= 0) {
            return cefCode;
        }
    }

    QCoreApplication app(argc, argv);

    const QString page = resolveMapPage();
    if (page.isEmpty() && !qEnvironmentVariableIsSet("MAP_PAGE_TEST_URL")) {
        std::printf("FAIL map.html not found\n");
        return 1;
    }
    const QString pageUrl = qEnvironmentVariableIsSet("MAP_PAGE_TEST_URL")
                                ? QString::fromLocal8Bit(qgetenv("MAP_PAGE_TEST_URL"))
                                : toFileUrl(page);
    std::printf("map page: %s\n", pageUrl.toStdString().c_str());

    HWND hostWindow = createHostWindow();
    if (!hostWindow) {
        std::printf("FAIL could not create host window\n");
        return 1;
    }

    const QString exeDir = QCoreApplication::applicationDirPath();
    CefMainArgs mainArgs(GetModuleHandle(nullptr));

    CefSettings settings;
    settings.no_sandbox = true;
    settings.multi_threaded_message_loop = true;
       settings.log_severity = LOGSEVERITY_WARNING;
    CefString(&settings.log_file) = (exeDir + "/cef_map_page_test.log").toStdString();
    CefString(&settings.resources_dir_path) = exeDir.toStdString();
    CefString(&settings.locales_dir_path) = (exeDir + "/locales").toStdString();
    CefString(&settings.root_cache_path) = (exeDir + "/cache").toStdString();

    CefRefPtr<CefApp> cefApp(new TestApp());
    if (!CefInitialize(mainArgs, settings, cefApp.get(), nullptr)) {
        std::printf("FAIL CefInitialize failed\n");
        return 1;
    }

    CefRefPtr<TestClient> client(new TestClient());
    const std::string url = pageUrl.toStdString();
    CefPostTask(TID_UI, base::BindOnce(
        [](CefRefPtr<TestClient> client, std::string url, HWND host) {
            std::printf("UI task running" );
            if (!client) {
                return;
            }
            CefWindowInfo info;
            info.SetAsChild(host, CefRect(0, 0, 800, 600));
            CefBrowserSettings browserSettings;
            if (!CefBrowserHost::CreateBrowser(info, client, CefString(url),
                                               browserSettings, nullptr, nullptr)) {
                std::printf("FAIL CreateBrowser failed\n");
                g_step = static_cast<int>(Step::kDone);
            }
        },
        client, url, hostWindow));

    // The browser process main thread must keep pumping messages, otherwise the
    // renderer cannot connect back to the browser process.
    QEventLoop loop;
    QTimer timer;
    timer.setInterval(100);
    QObject::connect(&timer, &QTimer::timeout, &loop, [&]() {
        if (g_probeReceived.load()) {
            loop.quit();
        }
    });
    timer.start();
    QTimer::singleShot(30000, &loop, &QEventLoop::quit);
    loop.exec();

    if (g_step.load() < static_cast<int>(Step::kProbe)) {
        std::printf("FAIL page did not load\n");
        return 1;
    }
    std::printf("PASS page loaded\n");

    {
        QJsonObject probe;
        if (!rustcam::cefbridge::parseBridgeMessage(QString::fromStdString(g_bridgePayload),
                                                    &probe)) {
            std::printf("FAIL probe payload is invalid\n");
            return 1;
        }
        const QString bridge = probe.value(QStringLiteral("bridge")).toString();
        const QString lib = probe.value(QStringLiteral("lib")).toString();
        const QString gl = probe.value(QStringLiteral("gl")).toString();
        std::printf("WebGL: %s\n", gl.toStdString().c_str());
        const QString canPlay264 = probe.value(QStringLiteral("canPlay264")).toString();
        std::printf("H264: mse=%d aac=%d canPlay='%s' flvjs=%s\n",
                    probe.value(QStringLiteral("mse264")).toBool() ? 1 : 0,
                    probe.value(QStringLiteral("mseAac")).toBool() ? 1 : 0,
                    canPlay264.toStdString().c_str(),
                    probe.value(QStringLiteral("flvjs")).toString().toStdString().c_str());
        if (bridge != QStringLiteral("function")) {
            std::printf("FAIL window.rustcamNative is %s\n", bridge.toStdString().c_str());
            return 1;
        }
        if (lib != QStringLiteral("loaded")) {
            std::printf("FAIL maplibre-gl.js is %s\n", lib.toStdString().c_str());
            return 1;
        }
        if (!gl.startsWith(QStringLiteral("webgl2"))) {
            std::printf("FAIL WebGL2 is required by maplibre-gl v4\n");
            return 1;
        }
    }
    std::printf("PASS native bridge injected and maplibre loaded from local assets\n");

    // C++ -> JS: ask the page to report a marker click.
    CefPostTask(TID_UI, base::BindOnce(
        [](CefRefPtr<TestClient> client) {
            if (!client) {
                return;
            }
            CefRefPtr<CefBrowser> browser = client->browser();
            if (!browser) {
                return;
            }
            CefRefPtr<CefFrame> frame = browser->GetMainFrame();
            if (!frame) {
                return;
            }
            const std::string script =
                "sendToNative({ type: 'markerClicked', id: 'test-device-1' });";
            frame->ExecuteJavaScript(script, frame->GetURL().ToString(), 0);
        },
        client));

    QEventLoop markerLoop;
    QTimer markerTimer;
    markerTimer.setInterval(100);
    QObject::connect(&markerTimer, &QTimer::timeout, &markerLoop, [&]() {
        if (g_step.load() >= static_cast<int>(Step::kMarker)) {
            markerLoop.quit();
        }
    });
    markerTimer.start();
    QTimer::singleShot(20000, &markerLoop, &QEventLoop::quit);
    markerLoop.exec();

    if (g_step.load() < static_cast<int>(Step::kMarker)) {
        std::printf("FAIL no marker message received\n");
        return 1;
    }
    std::printf("bridge payload: %s\n", g_bridgePayload.c_str());

    {
        QJsonObject parsed;
        if (!rustcam::cefbridge::parseBridgeMessage(QString::fromStdString(g_bridgePayload),
                                                    &parsed) ||
            parsed.value(QStringLiteral("type")).toString() != QStringLiteral("markerClicked") ||
            parsed.value(QStringLiteral("id")).toString() != QStringLiteral("test-device-1")) {
            std::printf("FAIL bridge payload has unexpected content\n");
            return 1;
        }
    }
    std::printf("PASS bridge message received and parsed\n");

    std::printf("MAP PAGE TEST OK\n");
    return 0;
}
