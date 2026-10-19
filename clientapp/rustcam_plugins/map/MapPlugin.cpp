#include "MapPlugin.h"
#include "core/EventBus.h"
#include "core/ServerApi.h"
#include "common/Logger.h"
#include <QWidget>
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QToolBar>
#include <QLabel>
#include <QPushButton>
#include <QFrame>
#include <QLineEdit>
#include <QListWidget>
#include <QListWidgetItem>
#include <QToolButton>

#ifdef RUSTCAM_WITH_WEBENGINE
#include "core/MapView.h"
#endif

namespace rustcam {

bool MapPlugin::initialize(EventBus* bus, QObject* cfg)
{
    Q_UNUSED(bus)
    Q_UNUSED(cfg)

    LOG_INFO("MapPlugin", "Initialized");
    return true;
}

void MapPlugin::shutdown()
{
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

    QLabel* titleLabel = new QLabel(tr("Electronic Map"), toolbar);
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

#ifdef RUSTCAM_WITH_WEBENGINE
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
#endif

    QToolButton* locateBtn = new QToolButton(toolbar);
    locateBtn->setText(tr("Locate"));
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
    fullscreenBtn->setText(tr("Fullscreen"));
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

    m_mapSidePanel = new MapSidePanel(m_workspace);
    connect(m_mapSidePanel, &MapSidePanel::deviceSelected, this, &MapPlugin::onDeviceSelected);

#ifdef RUSTCAM_WITH_WEBENGINE
    QVBoxLayout* mapLayout = qobject_cast<QVBoxLayout*>(m_mapSidePanel->mapContainer()->layout());
    if (mapLayout) {
        MapView* mapView = new MapView(m_mapSidePanel->mapContainer());
        mapView->initialize(116.4, 39.9, 10);
        mapLayout->addWidget(mapView);
    }
#endif

    layout->addWidget(m_mapSidePanel, 1);

    if (DataManager::instance()->isLoaded()) {
        onDataManagerLoaded();
    } else {
        connect(DataManager::instance(), &DataManager::loadFinished, this, &MapPlugin::onDataManagerLoaded, Qt::UniqueConnection);
    }

    loadDeviceMarkers();

    return m_workspace;
}

QWidget* MapPlugin::createSidePanel(QWidget* parent)
{
    QWidget* sidePanel = new QWidget(parent);
    sidePanel->setStyleSheet("background-color: #2b2b2b;");

    QVBoxLayout* layout = new QVBoxLayout(sidePanel);
    layout->setContentsMargins(8, 8, 8, 8);
    layout->setSpacing(8);

    QLabel* titleLabel = new QLabel(tr("Device List"), sidePanel);
    titleLabel->setStyleSheet(R"(
        QLabel {
            color: #e0e0e0;
            font-size: 14px;
            font-weight: bold;
        }
    )");
    layout->addWidget(titleLabel);

    QLineEdit* searchEdit = new QLineEdit(sidePanel);
    searchEdit->setPlaceholderText(tr("Search device..."));
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

    QListWidget* deviceList = new QListWidget(sidePanel);
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

    layout->addWidget(deviceList, 1);

    return sidePanel;
}

void MapPlugin::onDataManagerLoaded() {
    disconnect(DataManager::instance(), &DataManager::loadFinished, this, &MapPlugin::onDataManagerLoaded);

    if (m_mapSidePanel) {
        m_mapSidePanel->setRegionTreeNodes(DataManager::instance()->regionTree());
        m_mapSidePanel->setGroupTreeNodes(DataManager::instance()->groupTree());
        m_mapSidePanel->setDeviceTreeNodes(DataManager::instance()->deviceList());
    }

    LOG_INFO("MapPlugin", QString("Using DataManager: regionNodes=%1, groupNodes=%2, deviceNodes=%3")
        .arg(DataManager::instance()->regionTree().size())
        .arg(DataManager::instance()->groupTree().size())
        .arg(DataManager::instance()->deviceList().size()));
}

void MapPlugin::loadDeviceMarkers()
{
    ServerApi::instance()->get("/api/v1/devices", [this](bool success, const QJsonObject& resp, const QString&) {
        if (success) {
            QJsonArray devices = resp["data"].toArray();
            for (const QJsonValue& dev : devices) {
                QJsonObject obj = dev.toObject();
                if (obj.contains("lat") && obj.contains("lon")) {
                    MapMarkerData marker;
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

void MapPlugin::addMarker(const MapMarkerData& marker)
{
    m_markers[marker.id] = marker;
    LOG_INFO("MapPlugin", QString("Marker added: %1 at %2,%3").arg(marker.name).arg(marker.lat).arg(marker.lon));
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
#ifdef RUSTCAM_WITH_WEBENGINE
    QVBoxLayout* mapLayout = qobject_cast<QVBoxLayout*>(m_mapSidePanel->mapContainer()->layout());
    if (mapLayout) {
        QLayoutItem* item = mapLayout->itemAt(0);
        if (item && item->widget()) {
            MapView* mapView = qobject_cast<MapView*>(item->widget());
            if (mapView) {
                mapView->setCenter(lon, lat, zoom);
            }
        }
    }
#endif
    LOG_INFO("MapPlugin", QString("Focus on: %1, %2 zoom %3").arg(lat).arg(lon).arg(zoom));
}

void MapPlugin::onDeviceSelected(const QString& deviceId, const QString& deviceName)
{
    Q_UNUSED(deviceId)
    Q_UNUSED(deviceName)
}

}
