#include "CefBrowserWidget.h"

#include "core/BrowserHost.h"
#include "core/platform/BrowserWindowHost.h"
#include "common/CefBridgeProtocol.h"
#include "common/Logger.h"

#include <QJsonObject>
#include <QHideEvent>
#include <QResizeEvent>
#include <QShowEvent>
#include <QUuid>

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

} // namespace

CefBrowserWidget::CefBrowserWidget(QWidget* parent)
    : QWidget(parent)
{
    setAttribute(Qt::WA_NativeWindow);
    setMinimumSize(200, 150);

    BrowserHost* host = BrowserHost::instance();
    connect(host, &BrowserHost::sessionCreated,
            this, &CefBrowserWidget::onSessionCreated);
    connect(host, &BrowserHost::sessionReady,
            this, &CefBrowserWidget::onSessionReady);
    connect(host, &BrowserHost::sessionEvent,
            this, &CefBrowserWidget::onSessionEvent);
    connect(host, &BrowserHost::sessionFailed,
            this, &CefBrowserWidget::onSessionFailed);
}

CefBrowserWidget::~CefBrowserWidget()
{
    if (!m_sessionId.isEmpty()) {
        detachRemoteWindow();
        BrowserHost::instance()->closeSession(m_sessionId);
        m_sessionId.clear();
    }
    m_remoteHandle = 0;
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
        return;
    }
    // 切走再切回地图页时，hideEvent() 已把远端窗口隐藏；这里必须重新应用
    // 几何并显示，否则页面仍在运行（mapReady/瓦片正常）但屏幕上是空白。
    applyRemoteBounds();
}

void CefBrowserWidget::hideEvent(QHideEvent* event)
{
    QWidget::hideEvent(event);
    if (!m_remoteHandle) {
        return;
    }
    if (m_overlayMode) {
        BrowserHost::instance()->setSessionVisible(m_sessionId, false);
        return;
    }
    BrowserWindowHost* wh = createBrowserWindowHost();
    if (wh) {
        wh->setVisible(static_cast<WId>(m_remoteHandle), false);
        delete wh;
    }
}

void CefBrowserWidget::createBrowser()
{
    if (m_browserCreated || m_pageUrl.isEmpty()) {
        return;
    }

    BrowserWindowHost* wh = createBrowserWindowHost();
    m_overlayMode = (wh == nullptr) || !wh->supportsEmbedding();
    delete wh;

    const WId hostHandle = winId();
    if (!m_overlayMode && !hostHandle) {
        emit loadFailed(QStringLiteral("no native window handle"));
        return;
    }

    m_sessionId = QStringLiteral("map-%1")
                       .arg(QUuid::createUuid().toString(QUuid::WithoutBraces));

    BrowserHost::SessionRequest request;
    request.sessionId = m_sessionId;
    request.url = m_pageUrl;
    request.hostHandle = m_overlayMode ? 0 : hostHandle;
    request.mode = m_overlayMode ? QStringLiteral("overlay")
                                  : QStringLiteral("embedded");

    if (!BrowserHost::instance()->createSession(request)) {
        emit loadFailed(QStringLiteral("browser session unavailable"));
        m_sessionId.clear();
        return;
    }
    m_browserCreated = true;
}

void CefBrowserWidget::onSessionCreated(const QString& sessionId, quint64 handle)
{
    if (sessionId != m_sessionId) {
        return;
    }
    // 同一 session id 的会话被重建（browser 进程崩溃重启）：复位页面状态。
    m_remoteHandle = handle;
    m_pageLoaded = false;
    m_mapReady = false;
    m_initSent = false;
    applyRemoteBounds();
}

void CefBrowserWidget::onSessionReady(const QString& sessionId)
{
    if (sessionId != m_sessionId) {
        return;
    }
    handleLoadEnd();
}

void CefBrowserWidget::onSessionEvent(const QString& sessionId, const QString& payload)
{
    if (sessionId != m_sessionId) {
        return;
    }
    handleBridgeMessage(payload);
}

void CefBrowserWidget::onSessionFailed(const QString& sessionId, const QString& error)
{
    if (sessionId != m_sessionId) {
        return;
    }
    emit loadFailed(error);
}

void CefBrowserWidget::resizeEvent(QResizeEvent* event)
{
    QWidget::resizeEvent(event);
    applyRemoteBounds();
}

void CefBrowserWidget::applyRemoteBounds()
{
    if (!m_remoteHandle) {
        return;
    }

    if (m_overlayMode) {
        // overlay 模式：把控件的屏幕矩形下发给 browser 进程，
        // 由它移动自己的顶层窗口。
        // mapToGlobal 只接受点，取左上角再拼上尺寸。
        const QPoint topLeft = mapToGlobal(QPoint(0, 0));
        BrowserHost::instance()->setSessionBounds(m_sessionId,
                                                   topLeft.x(), topLeft.y(),
                                                   width(), height());
        BrowserHost::instance()->setSessionVisible(m_sessionId, isVisible());
        return;
    }

    BrowserWindowHost* wh = createBrowserWindowHost();
    if (!wh) {
        return;
    }
    const WId remote = static_cast<WId>(m_remoteHandle);
    if (wh->isValid(remote)) {
        wh->setBounds(remote, rect());
        wh->setVisible(remote, isVisible());
    }
    delete wh;
}

void CefBrowserWidget::detachRemoteWindow()
{
    if (!m_remoteHandle || m_overlayMode) {
        m_remoteHandle = 0;
        return;
    }
    BrowserWindowHost* wh = createBrowserWindowHost();
    if (wh) {
        wh->detach(static_cast<WId>(m_remoteHandle));
        delete wh;
    }
    m_remoteHandle = 0;
}

void CefBrowserWidget::executeJavaScript(const QString& script)
{
    if (!m_pageLoaded || m_sessionId.isEmpty()) {
        m_pendingScripts.append(qMakePair(QString(), script));
        return;
    }
    BrowserHost::instance()->executeJavaScript(m_sessionId, script);
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
    applyRemoteBounds();
    // 页面每次加载都要重新下发 initMap：browser 进程崩溃重启后会话重建，
    // 新页面没有收到过初始化指令（spec §7.1）。
    if (!m_initScript.isEmpty() && !m_initSent) {
        m_initSent = true;
        BrowserHost::instance()->executeJavaScript(m_sessionId, m_initScript);
    }
    flushPendingScripts();
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

void CefBrowserWidget::initialize(double lon, double lat, int zoom, const QString& layersJson)
{
    // jsonEscape 已转义 \ " ' 与换行，所以 JSON 串放进单引号里是安全的 JS 字符串字面量。
    m_initScript = QStringLiteral("initMap(%1, %2, %3, '%4')")
        .arg(lon).arg(lat).arg(zoom).arg(jsonEscape(layersJson));
    if (m_pageLoaded && !m_initSent) {
        m_initSent = true;
        BrowserHost::instance()->executeJavaScript(m_sessionId, m_initScript);
    }
    // 页面还没加载完时，由 handleLoadEnd() 在 OnLoadEnd 之后下发。
}

void CefBrowserWidget::setTilesUrl(const QString& layersJson)
{
    executeJavaScript(QStringLiteral("setTilesUrl('%1')").arg(jsonEscape(layersJson)));
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
