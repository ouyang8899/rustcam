#include "MapPlugin.h"
#include "core/EventBus.h"
#include "core/ServerApi.h"
#include "core/Logger.h"
#include <QWidget>
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QToolBar>
#include <QLabel>
#include <QPushButton>
#include <QFrame>
#include <QLineEdit>
#include <QListWidget>
#include <QToolButton>

namespace rustcam {

bool MapPlugin::initialize(EventBus* bus, QObject* cfg)
{
    Q_UNUSED(cfg)

    EventBus::instance()->subscribe("device/online", this, SLOT(onDeviceOnline(QString,QVariantMap)));
    EventBus::instance()->subscribe("device/offline", this, SLOT(onDeviceOffline(QString,QVariantMap)));
    EventBus::instance()->subscribe("alarm/triggered", this, SLOT(onAlarmTriggered(QString,QVariantMap)));
    EventBus::instance()->subscribe("map/focus", this, SLOT(onMapFocus(QString,QVariantMap)));
    EventBus::instance()->subscribe("map/marker/add", this, SLOT(onMarkerAdd(QString,QVariantMap)));

    loadDeviceMarkers();

    LOG_INFO("MapPlugin", "Initialized");
    return true;
}

void MapPlugin::shutdown()
{
    EventBus::instance()->unsubscribeAll(this);
}

QWidget* MapPlugin::createWorkspace(QWidget* parent)
{
    m_workspace = new QWidget(parent);
    m_workspace->setObjectName("workspaceArea");
    m_workspace->setStyleSheet(R"(
        QWidget#workspaceArea {
            background-color: #2b2b2b;
        }
    )");

    QVBoxLayout* layout = new QVBoxLayout(m_workspace);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    QToolBar* toolbar = new QToolBar(m_workspace);
    toolbar->setStyleSheet(R"(
        QToolBar {
            background-color: #1e1e1e;
            border: none;
            spacing: 8px;
            padding: 4px;
        }
    )");

    QLabel* titleLabel = new QLabel("电子地图", toolbar);
    titleLabel->setStyleSheet(R"(
        QLabel {
            color: #e0e0e0;
            font-size: 14px;
            font-weight: bold;
            padding: 0 8px;
        }
    )");
    toolbar->addWidget(titleLabel);

    toolbar->addSeparator();

    QToolButton* zoomInBtn = new QToolButton(toolbar);
    zoomInBtn->setText("+");
    zoomInBtn->setCursor(Qt::PointingHandCursor);
    zoomInBtn->setStyleSheet(R"(
        QToolButton {
            background-color: #2b2b2b;
            color: #e0e0e0;
            border: 1px solid #3e3e3e;
            border-radius: 4px;
            min-width: 32px;
        }
        QToolButton:hover {
            background-color: #3e3e3e;
        }
    )");
    toolbar->addWidget(zoomInBtn);

    QToolButton* zoomOutBtn = new QToolButton(toolbar);
    zoomOutBtn->setText("-");
    zoomOutBtn->setCursor(Qt::PointingHandCursor);
    zoomOutBtn->setStyleSheet(R"(
        QToolButton {
            background-color: #2b2b2b;
            color: #e0e0e0;
            border: 1px solid #3e3e3e;
            border-radius: 4px;
            min-width: 32px;
        }
        QToolButton:hover {
            background-color: #3e3e3e;
        }
    )");
    toolbar->addWidget(zoomOutBtn);

    toolbar->addSeparator();

    QToolButton* locateBtn = new QToolButton(toolbar);
    locateBtn->setText("定位");
    locateBtn->setCursor(Qt::PointingHandCursor);
    locateBtn->setStyleSheet(R"(
        QToolButton {
            background-color: #2b2b2b;
            color: #e0e0e0;
            border: 1px solid #3e3e3e;
            border-radius: 4px;
            padding: 4px 12px;
        }
        QToolButton:hover {
            background-color: #3e3e3e;
        }
    )");
    toolbar->addWidget(locateBtn);

    toolbar->addSeparator();

    QToolButton* fullscreenBtn = new QToolButton(toolbar);
    fullscreenBtn->setText("全屏");
    fullscreenBtn->setCursor(Qt::PointingHandCursor);
    fullscreenBtn->setStyleSheet(R"(
        QToolButton {
            background-color: #2b2b2b;
            color: #e0e0e0;
            border: 1px solid #3e3e3e;
            border-radius: 4px;
            padding: 4px 12px;
        }
        QToolButton:hover {
            background-color: #3e3e3e;
        }
    )");
    toolbar->addWidget(fullscreenBtn);

    layout->addWidget(toolbar);

    QFrame* mapFrame = new QFrame(m_workspace);
    mapFrame->setStyleSheet(R"(
        QFrame {
            background-color: #1a2a3a;
            border: 1px solid #3e3e3e;
        }
    )");
    QVBoxLayout* mapLayout = new QVBoxLayout(mapFrame);
    mapLayout->setAlignment(Qt::AlignCenter);

    QLabel* mapPlaceholder = new QLabel("地图区域\n\n支持百度/高德/离线地图", mapFrame);
    mapPlaceholder->setAlignment(Qt::AlignCenter);
    mapPlaceholder->setStyleSheet(R"(
        QLabel {
            color: #5a7a9a;
            font-size: 18px;
            background-color: transparent;
        }
    )");
    mapLayout->addWidget(mapPlaceholder);

    for (auto it = m_markers.begin(); it != m_markers.end(); ++it) {
        const MapMarker& marker = it.value();
        QLabel* markerLabel = new QLabel(mapFrame);
        markerLabel->setText(QString("<div style='background:%1;width:12px;height:12px;border-radius:50%;display:inline-block;'></div> %2")
                            .arg(marker.online ? "#4caf50" : "#f44336")
                            .arg(marker.name));
        markerLabel->setStyleSheet("color: #e0e0e0; font-size: 12px; background: rgba(0,0,0,0.5); padding: 4px 8px; border-radius: 4px;");
        mapLayout->addWidget(markerLabel);
    }

    layout->addWidget(mapFrame, 1);

    return m_workspace;
}

QWidget* MapPlugin::createSidePanel(QWidget* parent)
{
    m_sidePanel = new QWidget(parent);
    m_sidePanel->setStyleSheet("background-color: #2b2b2b;");

    QVBoxLayout* layout = new QVBoxLayout(m_sidePanel);
    layout->setContentsMargins(8, 8, 8, 8);
    layout->setSpacing(8);

    QLabel* titleLabel = new QLabel("设备列表", m_sidePanel);
    titleLabel->setStyleSheet(R"(
        QLabel {
            color: #e0e0e0;
            font-size: 14px;
            font-weight: bold;
        }
    )");
    layout->addWidget(titleLabel);

    QLineEdit* searchEdit = new QLineEdit(m_sidePanel);
    searchEdit->setPlaceholderText("搜索设备...");
    searchEdit->setStyleSheet(R"(
        QLineEdit {
            background-color: #1a1a1a;
            border: 1px solid #3e3e3e;
            border-radius: 4px;
            padding: 8px 12px;
            color: #e0e0e0;
        }
        QLineEdit:focus {
            border-color: #0078D4;
        }
    )");
    layout->addWidget(searchEdit);

    QListWidget* deviceList = new QListWidget(m_sidePanel);
    deviceList->setStyleSheet(R"(
        QListWidget {
            background-color: #1a1a1a;
            border: 1px solid #3e3e3e;
            border-radius: 4px;
            color: #e0e0e0;
        }
        QListWidget::item {
            padding: 8px;
            border-bottom: 1px solid #2e2e2e;
        }
        QListWidget::item:selected {
            background-color: rgba(0, 120, 212, 0.3);
        }
    )");

    for (auto it = m_markers.begin(); it != m_markers.end(); ++it) {
        const MapMarker& marker = it.value();
        QListWidgetItem* item = new QListWidgetItem(marker.name, deviceList);
        item->setData(Qt::UserRole, marker.id);
    }

    layout->addWidget(deviceList, 1);

    return m_sidePanel;
}

void MapPlugin::addMarker(const MapMarker& marker)
{
    m_markers[marker.id] = marker;
    LOG_INFO("MapPlugin", QString("Marker added: %1").arg(marker.name));
}

void MapPlugin::removeMarker(const QString& markerId)
{
    if (m_markers.remove(markerId) > 0) {
        LOG_INFO("MapPlugin", QString("Marker removed: %1").arg(markerId));
    }
}

void MapPlugin::updateMarkerPosition(const QString& markerId, double lat, double lon)
{
    if (m_markers.contains(markerId)) {
        m_markers[markerId].lat = lat;
        m_markers[markerId].lon = lon;
        LOG_DEBUG("MapPlugin", QString("Marker %1 position updated: %2, %3").arg(markerId).arg(lat).arg(lon));
    }
}

void MapPlugin::focusOn(double lat, double lon, int zoom)
{
    Q_UNUSED(lat)
    Q_UNUSED(lon)
    Q_UNUSED(zoom)
    LOG_INFO("MapPlugin", QString("Focus on: %1, %2 zoom %3").arg(lat).arg(lon).arg(zoom));
}

void MapPlugin::loadDeviceMarkers()
{
    ServerApi::instance()->get("/api/v1/devices", [this](bool success, const QJsonObject& resp, const QString&) {
        if (success) {
            QJsonArray devices = resp["data"].toArray();
            for (const auto& dev : devices) {
                QJsonObject obj = dev.toObject();
                if (obj.contains("lat") && obj.contains("lon")) {
                    MapMarker marker;
                    marker.id = obj["id"].toString();
                    marker.name = obj["name"].toString();
                    marker.lat = obj["lat"].toDouble();
                    marker.lon = obj["lon"].toDouble();
                    marker.type = obj["type"].toString();
                    marker.online = obj["online"].toBool();
                    addMarker(marker);
                }
            }
        }
    });
}

void MapPlugin::onDeviceOnline(const QString& topic, const QVariantMap& payload)
{
    Q_UNUSED(topic)
    QString deviceId = payload["deviceId"].toString();
    if (m_markers.contains(deviceId)) {
        m_markers[deviceId].online = true;
        LOG_INFO("MapPlugin", QString("Device online: %1").arg(deviceId));
    }
}

void MapPlugin::onDeviceOffline(const QString& topic, const QVariantMap& payload)
{
    Q_UNUSED(topic)
    QString deviceId = payload["deviceId"].toString();
    if (m_markers.contains(deviceId)) {
        m_markers[deviceId].online = false;
        LOG_INFO("MapPlugin", QString("Device offline: %1").arg(deviceId));
    }
}

void MapPlugin::onAlarmTriggered(const QString& topic, const QVariantMap& payload)
{
    Q_UNUSED(topic)
    QString deviceId = payload["deviceId"].toString();
    double lat = payload["lat"].toDouble();
    double lon = payload["lon"].toDouble();
    if (!deviceId.isEmpty() && lat != 0 && lon != 0) {
        focusOn(lat, lon, 18);
        LOG_INFO("MapPlugin", QString("Alarm focus: %1 at %2, %3").arg(deviceId).arg(lat).arg(lon));
    }
}

void MapPlugin::onMapFocus(const QString& topic, const QVariantMap& payload)
{
    Q_UNUSED(topic)
    double lat = payload["lat"].toDouble();
    double lon = payload["lon"].toDouble();
    int zoom = payload["zoom"].toInt(15);
    focusOn(lat, lon, zoom);
}

void MapPlugin::onMarkerAdd(const QString& topic, const QVariantMap& payload)
{
    Q_UNUSED(topic)
    MapMarker marker;
    marker.id = payload["id"].toString();
    marker.name = payload["name"].toString();
    marker.lat = payload["lat"].toDouble();
    marker.lon = payload["lon"].toDouble();
    marker.type = payload["type"].toString();
    marker.online = payload["online"].toBool(true);
    addMarker(marker);
}

}

#include "MapPlugin.moc"
