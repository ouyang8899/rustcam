#include "MapPlugin.h"
#include "CefBrowserWidget.h"
#include "CefRuntime.h"
#include "core/EventBus.h"
#include "core/ServerApi.h"
#include "core/VideoPlayer.h"
#include "core/NotificationWidget.h"
#include "common/Logger.h"
#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QStringList>
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
#include <QSizePolicy>
#include <QDialog>
#include <QtConcurrent>
#include <QJsonArray>
#include <QUrl>
#include <QSharedPointer>

namespace rustcam {

namespace {

// Locates the directory that holds map.html.
//
// The plugin's runtime directory holds icons/ and plugin.json next to the
// DLL; web/ follows the same convention. The exe-relative candidates keep
// the deployed layout working as well.
QString resolveWebDirectory(const QString& pluginDllPath)
{
    QStringList candidates;
    const QString dllDir = QFileInfo(pluginDllPath).absolutePath();
    candidates << dllDir + "/web";
    candidates << dllDir + "/map/web";
    const QString exeDir = QCoreApplication::applicationDirPath();
    candidates << exeDir + "/rustcam_plugins/map/web";
    candidates << exeDir + "/resources/web";

    for (const QString& candidate : candidates) {
        if (QFileInfo::exists(candidate + "/map.html")) {
            return QDir(candidate).absolutePath();
        }
    }
    return QString();
}

} // namespace

bool MapPlugin::initialize(EventBus* bus, QObject* cfg)
{
    Q_UNUSED(cfg)

    if (!CefRuntime::instance().initialize()) {
        LOG_ERROR("MapPlugin", "Failed to initialize CEF runtime");
        return false;
    }

    // The tile url is served by the backend and arrives after login; the
    // plugin only subscribes here and applies it once it is known.
    if (bus) {
        m_tilesToken = bus->subscribe(TOPIC_MAP_TILES_URL, [this](const QString& topic, EventDataPtr data) {
            Q_UNUSED(topic)
            onTilesUrlReceived(data);
        });
    }

    LOG_INFO("MapPlugin", "Initialized");
    return true;
}

void MapPlugin::shutdown()
{
    if (!m_tilesToken.isEmpty()) {
        EventBus::instance()->unsubscribe(m_tilesToken);
        m_tilesToken.clear();
    }
    CefRuntime::instance().shutdown();
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

    QVBoxLayout* mapLayout = qobject_cast<QVBoxLayout*>(m_mapSidePanel->mapContainer()->layout());
    if (mapLayout) {
        m_mapReady = false;
        m_mapView = new CefBrowserWidget(m_mapSidePanel->mapContainer());
        m_mapView->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);
        m_mapView->setPageUrl(resolveMapPageUrl());
        connect(m_mapView, &CefBrowserWidget::markerClicked,
                this, &MapPlugin::onMarkerClicked);
        connect(m_mapView, &CefBrowserWidget::loadFailed, this,
                [](const QString& error) {
                    LOG_ERROR("MapPlugin", QString("Map view failed to load: %1").arg(error));
                });
        mapLayout->addWidget(m_mapView);
        connect(m_mapView, &CefBrowserWidget::mapReady, this, &MapPlugin::onMapReady);
        m_mapView->initialize(121.4737, 31.2304, 11, m_tmsUrl, m_minZoom, m_maxZoom);
    }

    connect(zoomInBtn, &QToolButton::clicked, this, [this]() {
        if (m_mapView) {
            m_mapView->zoomIn();
        }
    });
    connect(zoomOutBtn, &QToolButton::clicked, this, [this]() {
        if (m_mapView) {
            m_mapView->zoomOut();
        }
    });

    layout->addWidget(m_mapSidePanel, 1);

    if (DataManager::instance()->isLoaded()) {
        onDataManagerLoaded();
    } else {
        connect(DataManager::instance(), &DataManager::loadFinished, this, &MapPlugin::onDataManagerLoaded, Qt::UniqueConnection);
    }

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

QString MapPlugin::resolveMapPageUrl() const
{
    const QString webDir = resolveWebDirectory(m_pluginPath);
    if (webDir.isEmpty()) {
        LOG_ERROR("MapPlugin", "map.html not found; map view disabled");
        return QString();
    }
    QString path = QDir::toNativeSeparators(QDir(webDir).absoluteFilePath("map.html"));
    if (!path.startsWith(QLatin1Char('/'))) {
        path.prepend(QLatin1Char('/'));
    }
    return QStringLiteral("file:///") + path;
}

void MapPlugin::onDataManagerLoaded() {
    disconnect(DataManager::instance(), &DataManager::loadFinished, this, &MapPlugin::onDataManagerLoaded);

    if (m_mapSidePanel) {
        m_mapSidePanel->setRegionTreeNodes(DataManager::instance()->regionTree());
        m_mapSidePanel->setGroupTreeNodes(DataManager::instance()->groupTree());
        m_mapSidePanel->setDeviceTreeNodes(DataManager::instance()->deviceList());
    }

    loadDeviceMarkers();

    LOG_INFO("MapPlugin", QString("Using DataManager: regionNodes=%1, groupNodes=%2, deviceNodes=%3")
        .arg(DataManager::instance()->regionTree().size())
        .arg(DataManager::instance()->groupTree().size())
        .arg(DataManager::instance()->deviceList().size()));
}

void MapPlugin::loadDeviceMarkers()
{
    const QJsonArray devices = DataManager::instance()->devicesData();
    const QJsonArray channels = DataManager::instance()->channelsData();

    // 设备 -> 通道列表；通道对象按 device|channel 索引
    QMap<QString, QList<QString>> deviceChannels;
    QMap<QString, QJsonObject> channelObjects;
    for (const QJsonValue& value : channels) {
        const QJsonObject channel = value.toObject();
        const QString deviceTag = channel.value("device_tag").toString();
        const QString channelTag = channel.value("channel_tag").toString();
        if (deviceTag.isEmpty() || channelTag.isEmpty()) {
            continue;
        }
        deviceChannels[deviceTag].append(channelTag);
        channelObjects.insert(deviceTag + "|" + channelTag, channel);
    }

    for (const QJsonValue& value : devices) {
        const QJsonObject device = value.toObject();
        const QString deviceTag = device.value("device_tag").toString();
        if (deviceTag.isEmpty()) {
            continue;
        }

        const double lat = device.value("latitude").toDouble();
        const double lon = device.value("longitude").toDouble();
        if (lat == 0.0 && lon == 0.0) {
            continue;
        }

        const QList<QString> channelTags = deviceChannels.value(deviceTag);
        if (channelTags.isEmpty()) {
            MapMarkerData marker;
            marker.id = deviceTag + "|" + deviceTag;
            marker.name = device.value("name").toString(deviceTag);
            marker.lat = lat;
            marker.lon = lon;
            marker.type = device.value("device_type").toString();
            marker.status = device.value("status").toString();
            marker.online = marker.status.compare(QStringLiteral("Online"), Qt::CaseInsensitive) == 0;
            addMarker(marker);
            continue;
        }

        const int count = channelTags.size();
        for (int index = 0; index < count; ++index) {
            const QString channelTag = channelTags.at(index);
            const QJsonObject channel = channelObjects.value(deviceTag + "|" + channelTag);

            // 同一设备的多通道做轻微错位，避免完全重叠
            const double offsetLon = count > 1
                ? 0.00025 * (index - (count - 1) / 2.0)
                : 0.0;

            MapMarkerData marker;
            marker.id = deviceTag + "|" + channelTag;
            marker.name = channel.value("name").toString(deviceTag);
            marker.lat = lat;
            marker.lon = lon + offsetLon;
            marker.type = channel.value("device_type").toString(
                device.value("device_type").toString());
            marker.status = channel.value("status").toString(
                device.value("status").toString());
            marker.online = marker.status.compare(QStringLiteral("Online"), Qt::CaseInsensitive) == 0;
            addMarker(marker);
        }
    }

    LOG_INFO("MapPlugin", QString("Loaded %1 map markers").arg(m_markers.size()));
}

void MapPlugin::onMapReady()
{
    if (m_mapReady || !m_mapView) {
        return;
    }
    m_mapReady = true;

    // The map page is fully initialized now; push the markers that were
    // collected while the page was still loading.
    for (auto it = m_markers.constBegin(); it != m_markers.constEnd(); ++it) {
        const MapMarkerData& marker = it.value();
        m_mapView->addMarker(marker.id, marker.lon, marker.lat,
                             marker.name, marker.type, marker.online, marker.status);
    }
    LOG_INFO("MapPlugin", QString("Map ready, %1 markers sent").arg(m_markers.size()));
}

void MapPlugin::addMarker(const MapMarkerData& marker)
{
    m_markers[marker.id] = marker;
    if (m_mapReady && m_mapView) {
        m_mapView->addMarker(marker.id, marker.lon, marker.lat,
                             marker.name, marker.type, marker.online, marker.status);
    }
    LOG_INFO("MapPlugin", QString("Marker added: %1 at %2,%3").arg(marker.name).arg(marker.lat).arg(marker.lon));
}

void MapPlugin::removeMarker(const QString& markerId)
{
    if (m_markers.remove(markerId) > 0) {
        if (m_mapReady && m_mapView) {
            m_mapView->removeMarker(markerId);
        }
        LOG_INFO("MapPlugin", QString("Marker removed: %1").arg(markerId));
    }
}

void MapPlugin::updateMarkerPosition(const QString& markerId, double lat, double lon)
{
    if (m_markers.contains(markerId)) {
        m_markers[markerId].lat = lat;
        m_markers[markerId].lon = lon;
        if (m_mapReady && m_mapView) {
            const MapMarkerData& marker = m_markers[markerId];
            m_mapView->updateMarker(marker.id, marker.lon, marker.lat,
                                    marker.name, marker.type, marker.online, marker.status);
        }
        LOG_DEBUG("MapPlugin", QString("Marker %1 position updated: %2, %3").arg(markerId).arg(lat).arg(lon));
    }
}

void MapPlugin::focusOn(double lat, double lon, int zoom)
{
    if (m_mapView) {
        m_mapView->setCenter(lon, lat, zoom);
    }
    LOG_INFO("MapPlugin", QString("Focus on: %1, %2 zoom %3").arg(lat).arg(lon).arg(zoom));
}

void MapPlugin::onDeviceSelected(const QString& deviceId, const QString& deviceName)
{
    Q_UNUSED(deviceName)

    // 优先精确定位该设备的点位，其次按前缀匹配
    QString targetId;
    if (m_markers.contains(deviceId)) {
        targetId = deviceId;
    } else {
        for (auto it = m_markers.constBegin(); it != m_markers.constEnd(); ++it) {
            if (it.key().startsWith(deviceId + "|") || it.key().contains(deviceId)) {
                targetId = it.key();
                break;
            }
        }
    }

    if (targetId.isEmpty()) {
        LOG_INFO("MapPlugin", QString("No map marker for device: %1").arg(deviceId));
        return;
    }

    const MapMarkerData marker = m_markers.value(targetId);
    Q_UNUSED(marker)
    if (m_mapView) {
        m_mapView->locateMarker(targetId);
    }
}

void MapPlugin::onMarkerClicked(const QString& markerId)
{
    LOG_INFO("MapPlugin", QString("Marker clicked: %1").arg(markerId));
    emit markerClicked(markerId);

    const QStringList parts = markerId.split(QLatin1Char('|'));
    const QString deviceTag = parts.value(0);
    const QString channelTag = parts.size() > 1 ? parts.value(1) : deviceTag;
    if (deviceTag.isEmpty()) {
        return;
    }

    const MapMarkerData marker = m_markers.value(markerId);
    playCamera(deviceTag, channelTag, marker.name.isEmpty() ? deviceTag : marker.name);
}

void MapPlugin::playCamera(const QString& deviceTag, const QString& channelTag, const QString& name)
{
    const QString endpoint = QStringLiteral("/api/v1/channels/%1/%2/play-links")
                                 .arg(QString::fromUtf8(QUrl::toPercentEncoding(deviceTag)))
                                 .arg(QString::fromUtf8(QUrl::toPercentEncoding(channelTag)));

    // ServerApi 是同步实现，放到后台线程避免卡住界面。
    QtConcurrent::run([this, endpoint, name]() {
        QJsonObject response;
        QString error;
        const bool success = ServerApi::instance()->getSync(endpoint, response, error);
        QMetaObject::invokeMethod(this, [this, success, response, error, name]() {
            if (!success) {
                LOG_ERROR("MapPlugin", QString("Failed to get play links: %1").arg(error));
                EventBus::instance()->publish(TOPIC_NOTIFICATION_SHOW,
                    QSharedPointer<NotificationEventData>(new NotificationEventData(
                        tr("Failed to get play url: %1").arg(error), NotificationEventData::Error)));
                return;
            }

            const QJsonObject data = response.value(QStringLiteral("data")).toObject();
            QString url = data.value(QStringLiteral("rtsp_media")).toString();
            if (url.isEmpty()) url = data.value(QStringLiteral("hls")).toString();
            if (url.isEmpty()) url = data.value(QStringLiteral("flv")).toString();
            if (url.isEmpty()) {
                LOG_ERROR("MapPlugin", "No playable url in play links response");
                EventBus::instance()->publish(TOPIC_NOTIFICATION_SHOW,
                    QSharedPointer<NotificationEventData>(new NotificationEventData(
                        tr("No playable url available"), NotificationEventData::Warning)));
                return;
            }

            showPlayerDialog(name, url);
        }, Qt::QueuedConnection);
    });
}

void MapPlugin::showPlayerDialog(const QString& title, const QString& url)
{
    if (!m_playerDialog) {
        m_playerDialog = new QDialog(m_workspace);
        // 复用同一个窗口：关闭时只停止播放并隐藏，不销毁。
        m_playerDialog->setStyleSheet("QDialog { background-color: #1a1a1a; }");
        m_playerDialog->resize(960, 600);

        QVBoxLayout* layout = new QVBoxLayout(m_playerDialog);
        layout->setContentsMargins(0, 0, 0, 0);

        m_player = new VideoPlayer(m_playerDialog);
        layout->addWidget(m_player);

        connect(m_playerDialog, &QObject::destroyed, this, [this]() {
            m_player = nullptr;
        });
        connect(m_playerDialog, &QDialog::finished, this, [this](int) {
            if (m_player) {
                m_player->stop();
            }
        });
    }

    m_playerDialog->setWindowTitle(tr("Live Video - %1").arg(title));
    m_playerDialog->show();
    m_playerDialog->raise();
    m_playerDialog->activateWindow();

    if (m_player) {
        m_player->setDeviceName(title);
        m_player->play(url);
    }
}

void MapPlugin::onTilesUrlReceived(EventDataPtr data)
{
    if (!data) {
        return;
    }

    m_tmsUrl = data->m_payload.value("tms_url").toString();
    if (data->m_payload.contains("min_zoom")) {
        m_minZoom = data->m_payload.value("min_zoom").toInt();
    }
    if (data->m_payload.contains("max_zoom")) {
        m_maxZoom = data->m_payload.value("max_zoom").toInt();
    }
    if (m_minZoom > m_maxZoom) {
        m_minZoom = 0;
        m_maxZoom = 19;
    }
    LOG_INFO("MapPlugin", QString("Tile url received: %1 (zoom %2-%3)").arg(m_tmsUrl).arg(m_minZoom).arg(m_maxZoom));
    applyTilesUrl();
}

void MapPlugin::applyTilesUrl()
{
    if (!m_mapView) {
        // The workspace is created after login; the stored url is applied
        // there.
        return;
    }
    // 只切换底图，不改变当前视野。
    m_mapView->setTilesUrl(m_tmsUrl);
    m_mapView->setZoomLimits(m_minZoom, m_maxZoom);
}

}
