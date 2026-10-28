#include "CefBrowserWidget.h"
#include "CefRuntime.h"
#include "MapBrowserClient.h"
#include "common/CefBridgeProtocol.h"
#include "common/Logger.h"

#include "include/cef_browser.h"
#include "include/cef_frame.h"
#include "include/base/cef_bind.h"
#include "include/base/cef_callback.h"
#include "include/cef_task.h"
#include "include/wrapper/cef_closure_task.h"

#include <QJsonObject>
#include <QResizeEvent>
#include <QShowEvent>
#include <QTimer>

#include <windows.h>

namespace rustcam {

namespace {

QString jsonEscape(const QString& value)
{
    QString escaped = value;
    escaped.replace("\\", "\\\\");
    escaped.replace("\"", "\\\"");
    escaped.replace("'", "\\'");
    escaped.replace("\n", "\\n");
    escaped.replace("\r", "\\r");
    return escaped;
}

// CEF creates its child window with a class whose name can be either
// "Chrome_WidgetWin_0" or "Chrome_WidgetWin_1" depending on the CEF version.
// Match the prefix instead of a fixed name, otherwise the browser window is
// never found and stays at its initial (tiny) size.
HWND findCefChildWindow(HWND parent)
{
    struct Search {
        HWND result = nullptr;
    } search;

    EnumChildWindows(parent, [](HWND child, LPARAM param) -> BOOL {
        wchar_t className[64] = {};
        if (GetClassNameW(child, className, 64) > 0 &&
            wcsncmp(className, L"Chrome_WidgetWin_", 17) == 0) {
            reinterpret_cast<Search*>(param)->result = child;
            return FALSE;
        }
        return TRUE;
    }, reinterpret_cast<LPARAM>(&search));

    return search.result;
}

} // namespace

CefBrowserWidget::CefBrowserWidget(QWidget* parent)
    : QWidget(parent)
{
    setAttribute(Qt::WA_NativeWindow);
    setMinimumSize(200, 150);
}

CefBrowserWidget::~CefBrowserWidget()
{
    if (m_client) {
        m_client->setWidget(nullptr);
        if (CefRuntime::instance().isInitialized()) {
            m_client->closeBrowser();
        }
        CefRuntime::instance().removeClient(m_client.get());
        // The browser keeps its own reference to the client, so releasing ours
        // here is safe even while the browser is still closing.
        m_client = nullptr;
    }
}

void CefBrowserWidget::setPageUrl(const QString& url)
{
    m_pageUrl = url;
}

void CefBrowserWidget::showEvent(QShowEvent* event)
{
    QWidget::showEvent(event);
    if (!m_browserCreated) {
        createBrowser();
    }
}

void CefBrowserWidget::createBrowser()
{
    if (m_browserCreated || m_pageUrl.isEmpty()) {
        return;
    }
    if (!CefRuntime::instance().initialize()) {
        emit loadFailed(QStringLiteral("CEF runtime unavailable"));
        return;
    }

    const HWND parentHwnd = reinterpret_cast<HWND>(winId());
    if (!parentHwnd) {
        emit loadFailed(QStringLiteral("no native window handle"));
        return;
    }

    m_client = new MapBrowserClient(this);
    CefRuntime::instance().addClient(m_client.get());
    m_browserCreated = true;

    // Capture everything by value: the task runs on the CEF UI thread and may
    // outlive this widget.
    CefRefPtr<MapBrowserClient> client = m_client;
    const std::string url = m_pageUrl.toStdString();

    CefPostTask(TID_UI, base::BindOnce(
        [](CefRefPtr<MapBrowserClient> client, HWND parentHwnd, std::string url) {
            if (!client) {
                return;
            }
            CefWindowInfo windowInfo;
            RECT rect;
            GetClientRect(parentHwnd, &rect);
            windowInfo.SetAsChild(parentHwnd,
                                  CefRect(0, 0, rect.right - rect.left, rect.bottom - rect.top));

            CefBrowserSettings browserSettings;

            const bool created = CefBrowserHost::CreateBrowser(
                windowInfo, client, CefString(url), browserSettings, nullptr, nullptr);
            if (!created) {
                LOG_ERROR("CefBrowserWidget", "CreateBrowser failed");
            }
        },
        client, parentHwnd, url));
}

void CefBrowserWidget::resizeEvent(QResizeEvent* event)
{
    QWidget::resizeEvent(event);
    syncBrowserGeometry();
}

void CefBrowserWidget::syncBrowserGeometry()
{
    if (!m_client) {
        return;
    }
    const HWND parentHwnd = reinterpret_cast<HWND>(winId());
    if (!parentHwnd) {
        return;
    }

    HWND browserHwnd = static_cast<HWND>(m_browserWindow);
    if (!browserHwnd || !IsWindow(browserHwnd)) {
        // The browser window is created on the CEF UI thread; look it up as a
        // child of our native window.
        browserHwnd = findCefChildWindow(parentHwnd);
        if (!browserHwnd) {
            // Creation may still be in flight; retry shortly.
            QTimer::singleShot(100, this, [this]() { syncBrowserGeometry(); });
            return;
        }
        m_browserWindow = browserHwnd;
    }

    RECT rect;
    GetClientRect(parentHwnd, &rect);
    const int width = rect.right - rect.left;
    const int height = rect.bottom - rect.top;
    ::SetWindowPos(browserHwnd, nullptr, 0, 0, width, height,
                   SWP_NOZORDER | SWP_NOACTIVATE);

    CefRefPtr<MapBrowserClient> client = m_client;
    CefPostTask(TID_UI, base::BindOnce(
        [](CefRefPtr<MapBrowserClient> client) {
            if (client && client->browser()) {
                client->browser()->GetHost()->WasResized();
            }
        },
        client));

    if (m_mapReady) {
        executeJavaScript(QStringLiteral("if (typeof resize === 'function') resize();"));
    }
}

void CefBrowserWidget::executeJavaScript(const QString& script)
{
    if (!m_pageLoaded || !m_client) {
        m_pendingScripts.append(qMakePair(QString(), script));
        return;
    }

    CefRefPtr<MapBrowserClient> client = m_client;
    CefPostTask(TID_UI, base::BindOnce(
        [](CefRefPtr<MapBrowserClient> client, std::string script) {
            if (!client) {
                LOG_WARNING("CefBrowserWidget", "Script dropped: client is gone");
                return;
            }
            CefRefPtr<CefBrowser> browser = client->browser();
            if (!browser) {
                LOG_WARNING("CefBrowserWidget", "Script dropped: browser is null");
                return;
            }
            CefRefPtr<CefFrame> frame = browser->GetMainFrame();
            if (!frame) {
                LOG_WARNING("CefBrowserWidget", "Script dropped: main frame is null");
                return;
            }
            frame->ExecuteJavaScript(script, frame->GetURL().ToString(), 0);
        },
        client, script.toStdString()));
}

void CefBrowserWidget::flushPendingScripts()
{
    if (m_pendingScripts.isEmpty()) {
        return;
    }
    const auto pending = m_pendingScripts;
    m_pendingScripts.clear();
    for (const auto& entry : pending) {
        executeJavaScript(entry.second);
    }
}

void CefBrowserWidget::handleLoadEnd()
{
    m_pageLoaded = true;
    // The page may have been loaded after the last Qt resize event, so make
    // sure the native browser window matches the widget before running scripts.
    syncBrowserGeometry();
    flushPendingScripts();
    QTimer::singleShot(300, this, [this]() { syncBrowserGeometry(); });
}

void CefBrowserWidget::handleBridgeMessage(const QString& payload)
{
    QJsonObject message;
    if (!rustcam::cefbridge::parseBridgeMessage(payload, &message)) {
        LOG_WARNING("CefBrowserWidget", "Malformed bridge payload");
        return;
    }

    const QString type = message.value(QStringLiteral("type")).toString();
    if (type == QLatin1String(rustcam::cefbridge::event::kMapReady)) {
        m_mapReady = true;
        emit mapReady();
        return;
    }

    if (type == QLatin1String(rustcam::cefbridge::event::kMarkerClicked)) {
        const QString id = message.value(QStringLiteral("id")).toString();
        emit markerClicked(id);
        return;
    }
}

void CefBrowserWidget::initialize(double lon, double lat, int zoom, const QString& tmsUrl,
                                  int minZoom, int maxZoom)
{
    const QString script = QStringLiteral("initMap(%1, %2, %3, '%4', %5, %6)")
        .arg(lon).arg(lat).arg(zoom).arg(jsonEscape(tmsUrl))
        .arg(minZoom).arg(maxZoom);
    executeJavaScript(script);
}

void CefBrowserWidget::setTilesUrl(const QString& tmsUrl)
{
    executeJavaScript(QStringLiteral("setTilesUrl('%1')").arg(jsonEscape(tmsUrl)));
}

void CefBrowserWidget::setZoomLimits(int minZoom, int maxZoom)
{
    executeJavaScript(QStringLiteral("setZoomLimits(%1, %2)").arg(minZoom).arg(maxZoom));
}

void CefBrowserWidget::locateMarker(const QString& id)
{
    executeJavaScript(QStringLiteral("locateMarker('%1')").arg(jsonEscape(id)));
}

void CefBrowserWidget::setCenter(double lon, double lat, int zoom)
{
    QString script;
    if (zoom < 0) {
        script = QStringLiteral("setCenter(%1, %2)").arg(lon).arg(lat);
    } else {
        script = QStringLiteral("setCenter(%1, %2, %3)").arg(lon).arg(lat).arg(zoom);
    }
    executeJavaScript(script);
}

void CefBrowserWidget::addMarker(const QString& id, double lon, double lat,
                                 const QString& name, const QString& type,
                                 bool online, const QString& status)
{
    const QString script = QStringLiteral(
        "addMarker('%1', %2, %3, '%4', '%5', %6, '%7')")
        .arg(jsonEscape(id)).arg(lon).arg(lat)
        .arg(jsonEscape(name)).arg(jsonEscape(type))
        .arg(online ? QStringLiteral("true") : QStringLiteral("false"))
        .arg(jsonEscape(status));
    executeJavaScript(script);
}

void CefBrowserWidget::removeMarker(const QString& id)
{
    executeJavaScript(QStringLiteral("removeMarker('%1')").arg(jsonEscape(id)));
}

void CefBrowserWidget::updateMarker(const QString& id, double lon, double lat,
                                    const QString& name, const QString& type,
                                    bool online, const QString& status)
{
    const QString script = QStringLiteral(
        "updateMarker('%1', %2, %3, '%4', '%5', %6, '%7')")
        .arg(jsonEscape(id)).arg(lon).arg(lat)
        .arg(jsonEscape(name)).arg(jsonEscape(type))
        .arg(online ? QStringLiteral("true") : QStringLiteral("false"))
        .arg(jsonEscape(status));
    executeJavaScript(script);
}

void CefBrowserWidget::clearMarkers()
{
    executeJavaScript(QStringLiteral("clearMarkers()"));
}

void CefBrowserWidget::zoomIn()
{
    executeJavaScript(QStringLiteral("zoomIn()"));
}

void CefBrowserWidget::zoomOut()
{
    executeJavaScript(QStringLiteral("zoomOut()"));
}

void CefBrowserWidget::setZoom(int zoom)
{
    executeJavaScript(QStringLiteral("setZoom(%1)").arg(zoom));
}

} // namespace rustcam
