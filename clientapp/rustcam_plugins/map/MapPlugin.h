#pragma once

#include "interfaces/IPlugin.h"
#include <QObject>
#include <QString>
#include <QMap>
#include <QJsonObject>
#include <QPointer>
#include "MapSidePanel.h"
#include "core/DataManager.h"
#include "core/EventBus.h"

class QDialog;

namespace rustcam {

class CefBrowserWidget;
class VideoPlayer;

struct MapMarkerData {
    QString id;
    QString name;
    double lat = 0;
    double lon = 0;
    QString type;
    QString icon;
    bool online = false;
    QString status;
};

class MapPlugin : public IPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID "com.rustcam.IPlugin/1.0" FILE "plugin.json")
    Q_INTERFACES(rustcam::IPlugin)
public:
    QString id() const override { return "map"; }
    QString displayName() const override { return tr("电子地图"); }
    QString version() const override { return "1.0.0"; }

    bool initialize(EventBus* bus, QObject* cfg) override;
    void shutdown() override;
    QWidget* createWorkspace(QWidget* parent) override;
    QWidget* createSidePanel(QWidget* parent) override;
    bool isWorkspace() const override { return true; }

    void addMarker(const MapMarkerData& marker);
    void removeMarker(const QString& markerId);
    void updateMarkerPosition(const QString& markerId, double lat, double lon);
    void focusOn(double lat, double lon, int zoom = 15);

signals:
    void markerClicked(const QString& markerId);

private slots:
    void onDataManagerLoaded();
    void onMapReady();
    void onDeviceSelected(const QString& deviceId, const QString& deviceName);
    void onMarkerClicked(const QString& markerId);
    void onTilesUrlReceived(EventDataPtr data);
    void playCamera(const QString& deviceTag, const QString& channelTag, const QString& name);

private:
    void loadDeviceMarkers();
    QString resolveMapPageUrl() const;
    /// Sends the current tile url to the map page.
    void applyTilesUrl();
    void showPlayerDialog(const QString& title, const QString& url);

    QWidget* m_workspace;
    MapSidePanel* m_mapSidePanel;
    CefBrowserWidget* m_mapView = nullptr;
    QMap<QString, MapMarkerData> m_markers;
    /// True once the map page reported that its MapLibre instance is ready.
    bool m_mapReady = false;
    /// Tile url of the desktop client map, provided by the backend.
    QString m_tmsUrl;
    /// Zoom range of the desktop client map, provided by the backend.
    int m_minZoom = 0;
    int m_maxZoom = 19;
    /// Event bus subscription token for TOPIC_MAP_TILES_URL.
    QString m_tilesToken;
    /// Live video dialog opened from a map marker.
    QPointer<QDialog> m_playerDialog;
    VideoPlayer* m_player = nullptr;
};

}
