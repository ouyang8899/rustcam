#pragma once

#include "interfaces/IPlugin.h"
#include <QObject>
#include <QString>
#include <QMap>
#include <QJsonObject>

namespace rustcam {

struct MapMarker {
    QString id;
    QString name;
    double lat;
    double lon;
    QString type;
    QString icon;
    bool online;
};

class MapPlugin : public QObject, public IPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID "com.rustcam.IPlugin/1.0" FILE "plugin.json")
    Q_INTERFACES(rustcam::IPlugin)
public:
    QString id() const override { return "map"; }
    QString displayName() const override { return tr("电子地图"); }
    QString version() const override { return "1.0.0"; }
    QIcon icon() const override { return QIcon(":/icons/map.svg"); }

    bool initialize(EventBus* bus, QObject* cfg) override;
    void shutdown() override;
    QWidget* createWorkspace(QWidget* parent) override;
    QWidget* createSidePanel(QWidget* parent) override;
    bool isWorkspace() const override { return true; }

    void addMarker(const MapMarker& marker);
    void removeMarker(const QString& markerId);
    void updateMarkerPosition(const QString& markerId, double lat, double lon);
    void focusOn(double lat, double lon, int zoom = 15);

signals:
    void markerClicked(const QString& markerId);

private slots:
    void onDeviceOnline(const QString& topic, const QVariantMap& payload);
    void onDeviceOffline(const QString& topic, const QVariantMap& payload);
    void onAlarmTriggered(const QString& topic, const QVariantMap& payload);
    void onMapFocus(const QString& topic, const QVariantMap& payload);
    void onMarkerAdd(const QString& topic, const QVariantMap& payload);

private:
    void loadDeviceMarkers();

    QWidget* m_workspace;
    QWidget* m_sidePanel;
    QMap<QString, MapMarker> m_markers;
};

}
