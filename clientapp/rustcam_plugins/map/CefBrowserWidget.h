#pragma once

#include "include/cef_browser.h"

#include <QList>
#include <QPair>
#include <QString>
#include <QWidget>

namespace rustcam {

class MapBrowserClient;

// Qt widget that hosts a CEF browser as a native child window.
//
// The browser lives on the CEF UI thread (multi_threaded_message_loop = true),
// therefore every CEF call issued from the Qt thread is posted with
// CefPostTask(TID_UI, ...) and every CEF callback is bounced back to the Qt
// thread with QMetaObject::invokeMethod(..., Qt::QueuedConnection).
//
// Two readiness flags are tracked:
//   m_pageLoaded - the page finished loading, so ExecuteJavaScript() works.
//   m_mapReady   - the page reported mapReady through the native bridge.
class CefBrowserWidget : public QWidget {
    Q_OBJECT
public:
    explicit CefBrowserWidget(QWidget* parent = nullptr);
    ~CefBrowserWidget() override;

    void setPageUrl(const QString& url);

    void initialize(double lon, double lat, int zoom, const QString& tmsUrl = QString(),
                    int minZoom = 0, int maxZoom = 19);
    void setTilesUrl(const QString& tmsUrl);
    void setZoomLimits(int minZoom, int maxZoom);
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

private:
    void createBrowser();
    void executeJavaScript(const QString& script);
    void flushPendingScripts();
    void syncBrowserGeometry();

    CefRefPtr<MapBrowserClient> m_client;
    QString m_pageUrl;
    QList<QPair<QString, QString>> m_pendingScripts;
    void* m_browserWindow = nullptr; // HWND of the CEF child window
    bool m_browserCreated = false;
    bool m_pageLoaded = false;
    bool m_mapReady = false;
};

} // namespace rustcam
