#pragma once

#include <QList>
#include <QPair>
#include <QString>
#include <QWidget>

namespace rustcam {

/// 承载 rustcam-browser 进程中的 CEF browser 的 Qt 控件。
///
/// 浏览器窗口由 rustcam-browser 进程创建（CEF 拥有其生命周期），
/// 本控件通过跨进程 parent 把它挂进自己的原生窗口，并负责：
///   - 几何同步（SetWindowPos，跨进程）
///   - 显隐（ShowWindow，跨进程）
///   - 脚本下发（经 BrowserHost 走 IPC）
///   - 事件接收（经 BrowserHost 走 IPC）
///
/// 所有权边界（spec §5.1）：本控件**绝不**销毁远端窗口、**绝不**建立父子
/// 关系、**绝不**修改远端窗口样式；唯一允许的结构性操作是拆解时的
/// SetParent(remote, nullptr)。
class CefBrowserWidget : public QWidget {
    Q_OBJECT
public:
    explicit CefBrowserWidget(QWidget* parent = nullptr);
    ~CefBrowserWidget() override;

    void setPageUrl(const QString& url);

    void initialize(double lon, double lat, int zoom, const QString& layersJson = QString());
    void setTilesUrl(const QString& layersJson);
    void setCenter(double lon, double lat, int zoom = -1);
    void locateMarker(const QString& id);
    void addMarker(const QString& id, double lon, double lat,
                   const QString& name, const QString& type,
                   bool online, const QString& status);
    void removeMarker(const QString& id);
    void updateMarker(const QString& id, double lon, double lat,
                      const QString& name, const QString& type,
                      bool online, const QString& status);
    void clearMarkers();
    void zoomIn();
    void zoomOut();
    void setZoom(int zoom);

    // Called on the Qt thread.
    void handleLoadEnd();
    void handleBridgeMessage(const QString& payload);

signals:
    void markerClicked(const QString& markerId);
    void mapReady();
    void loadFailed(const QString& error);

protected:
    void showEvent(QShowEvent* event) override;
    void resizeEvent(QResizeEvent* event) override;
    void hideEvent(QHideEvent* event) override;

private:
    void createBrowser();
    void executeJavaScript(const QString& script);
    void flushPendingScripts();
    void applyRemoteBounds();
    void detachRemoteWindow();
    void onSessionCreated(const QString& sessionId, quint64 handle);
    void onSessionReady(const QString& sessionId);
    void onSessionEvent(const QString& sessionId, const QString& payload);
    void onSessionFailed(const QString& sessionId, const QString& error);

    QString m_pageUrl;
    QString m_sessionId;
    QList<QPair<QString, QString>> m_pendingScripts;
    quint64 m_remoteHandle = 0;   // 远端 CEF 窗口的原生句柄
    bool m_browserCreated = false;
    bool m_pageLoaded = false;
    bool m_mapReady = false;
    bool m_overlayMode = false;  // true = overlay 降级模式
    QString m_initScript;         // initMap 脚本；页面每次加载都要重新下发
    bool m_initSent = false;      // 本次页面加载是否已下发过 initMap
};

} // namespace rustcam
