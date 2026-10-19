#pragma once

#include "interfaces/IPlugin.h"
#include <QObject>
#include <QString>
#include <QMap>
#include <QJsonObject>
#include "core/MapSidePanel.h"
#include "core/DataManager.h"

namespace rustcam {

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
    void onDeviceSelected(const QString& deviceId, const QString& deviceName);

private:
    void loadDeviceMarkers();

    QWidget* m_workspace;
    MapSidePanel* m_mapSidePanel;
    QMap<QString, MapMarkerData> m_markers;
};

}
