#pragma once

#include "CoreExport.h"

#ifdef RUSTCAM_WITH_WEBENGINE
#include <QWidget>
#include <QWebEngineView>
#include <QJsonObject>
#include <QVariantMap>

namespace rustcam {

struct RUSTCAM_CORE_API MapMarker {
    QString id;
    QString name;
    double lat = 0;
    double lon = 0;
    QString type;
    bool online = false;
    QString status;
};

class RUSTCAM_CORE_API MapView : public QWidget {
    Q_OBJECT
public:
    explicit MapView(QWidget* parent = nullptr);
    ~MapView();

    void initialize(double centerLon = 116.4, double centerLat = 39.9, int zoom = 10);
    void setCenter(double lon, double lat, int zoom = -1);
    void addMarker(const MapMarker& marker);
    void removeMarker(const QString& markerId);
    void updateMarker(const MapMarker& marker);
    void clearMarkers();
    void zoomIn();
    void zoomOut();
    void setZoom(int zoom);

signals:
    void markerClicked(const QString& markerId);
    void mapReady();

private slots:
    void onJavaScriptReady();
    void handleJavaScriptResult(QVariant result);

private:
    void runJavaScript(const QString& script);

    QWebEngineView* m_webView;
    bool m_jsReady = false;
    QList<QPair<QString, QString>> m_pendingCalls;
};

}

#endif
