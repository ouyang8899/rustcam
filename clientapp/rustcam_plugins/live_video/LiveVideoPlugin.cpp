#include "LiveVideoPlugin.h"
#include "core/EventBus.h"
#include "core/ServerApi.h"
#include "core/DeviceTreeNode.h"
#include "common/Logger.h"
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QGridLayout>
#include <QStackedWidget>
#include <QToolBar>
#include <QAction>
#include <QComboBox>
#include <QJsonArray>

namespace rustcam {

VideoCell::VideoCell(const QString& cellId, QWidget* parent)
    : QWidget(parent)
    , m_cellId(cellId)
    , m_playing(false)
{
    setupUi();
}

void VideoCell::setTitle(const QString& title)
{
    m_titleLabel->setText(title);
}

void VideoCell::setPlaying(bool playing)
{
    m_playing = playing;
    if (playing) {
        m_overlayWidget->hide();
    } else {
        m_overlayWidget->show();
    }
}

void VideoCell::updateOsd(const QString& info)
{
    m_osdLabel->setText(info);
}

void VideoCell::onCellClicked()
{
    emit cellSelected(m_cellId);
}

void VideoCell::setupUi()
{
    setObjectName("VideoCell");
    setAttribute(Qt::WA_StyledBackground);

    QVBoxLayout* layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);

    m_videoLabel = new QLabel(this);
    m_videoLabel->setObjectName("VideoLabel");
    m_videoLabel->setAlignment(Qt::AlignCenter);
    m_videoLabel->setText(tr("NO SIGNAL"));
    layout->addWidget(m_videoLabel);

    m_overlayWidget = new QWidget(this);
    m_overlayWidget->setObjectName("VideoOverlay");
    QVBoxLayout* overlayLayout = new QVBoxLayout(m_overlayWidget);
    overlayLayout->setAlignment(Qt::AlignCenter);
    overlayLayout->setContentsMargins(0, 0, 0, 0);
    QLabel* iconLabel = new QLabel("📹", m_overlayWidget);
    iconLabel->setAlignment(Qt::AlignCenter);
    overlayLayout->addWidget(iconLabel);
    QLabel* textLabel = new QLabel(tr("Offline"), m_overlayWidget);
    textLabel->setObjectName("OverlayText");
    textLabel->setAlignment(Qt::AlignCenter);
    overlayLayout->addWidget(textLabel);

    m_osdLabel = new QLabel(this);
    m_osdLabel->setObjectName("OsdLabel");
    m_osdLabel->setAlignment(Qt::AlignLeft | Qt::AlignTop);
    m_osdLabel->hide();

    m_titleLabel = new QLabel(this);
    m_titleLabel->setObjectName("TitleLabel");
    m_titleLabel->setAlignment(Qt::AlignLeft | Qt::AlignBottom);
    m_titleLabel->hide();

    setMinimumSize(320, 180);
}

bool LiveVideoPlugin::initialize(EventBus* bus, QObject* cfg)
{
    Q_UNUSED(cfg)
    m_currentLayout = 4;
    m_maxCells = 16;
    m_videoContainer = nullptr;

    EventBus::instance()->subscribe(TOPIC_VIDEO_STREAM_REQUEST, [this](const QString& topic, EventDataPtr data) {
        onStreamRequested(topic, data);
    });
    EventBus::instance()->subscribe(TOPIC_VIDEO_STREAM_STOP, [this](const QString& topic, EventDataPtr data) {
        onStreamStop(topic, data);
    });
    m_sessionLoggedInToken = EventBus::instance()->subscribe(TOPIC_SESSION_LOGGED_IN, [this](const QString& topic, EventDataPtr data) {
        onLoggedIn(topic, data);
    });
    LOG_INFO("LiveVideoPlugin", QString("Subscribed to session/loggedIn"));

    LOG_INFO("LiveVideoPlugin", "Initialized");
    return true;
}

void LiveVideoPlugin::shutdown()
{
    EventBus::instance()->unsubscribeAll();
}

void LiveVideoPlugin::onLoggedIn(const QString& topic, EventDataPtr)
{
    Q_UNUSED(topic)
    qDebug() << "onLoggedIn invoked!";
    qDebug() << "m_regionTree:" << m_regionTree << "m_groupTree:" << m_groupTree << "m_videoContainer:" << m_videoContainer;

    if (m_regionTree) {
        loadRegionTree();
    }
    if (m_groupTree) {
        loadGroupTree();
    }
    if (m_videoContainer) {
        m_videoContainer->initialize();
    }
}

QWidget* LiveVideoPlugin::createWorkspace(QWidget* parent)
{
    QWidget* mainWidget = new QWidget(parent);
    mainWidget->setObjectName("LiveVideoWorkspace");

    QHBoxLayout* mainLayout = new QHBoxLayout(mainWidget);
    mainLayout->setContentsMargins(0, 0, 0, 0);
    mainLayout->setSpacing(4);

    m_sidePanel = new QWidget(mainWidget);
    m_sidePanel->setObjectName("sidePanel");
    m_sidePanel->setMinimumWidth(280);
    m_sidePanel->setMaximumWidth(400);
    QVBoxLayout* sideLayout = new QVBoxLayout(m_sidePanel);
    sideLayout->setContentsMargins(8, 8, 8, 8);
    sideLayout->setSpacing(4);

    m_tabWidget = new QTabWidget(m_sidePanel);

    m_regionTree = new DeviceTree(m_tabWidget);
    QObject::connect(m_regionTree, QOverload<QVariantMap>::of(&DeviceTree::itemDoubleClicked),
                     this, &LiveVideoPlugin::onDeviceTreeItemClicked);
    m_tabWidget->addTab(m_regionTree, tr("地区"));

    m_groupTree = new DeviceTree(m_tabWidget);
    QObject::connect(m_groupTree, QOverload<QVariantMap>::of(&DeviceTree::itemDoubleClicked),
                     this, &LiveVideoPlugin::onDeviceTreeItemClicked);
    m_tabWidget->addTab(m_groupTree, tr("自定义分组"));

    sideLayout->addWidget(m_tabWidget);

    m_videoContainer = new VideoPlayerContainer(mainWidget);

    mainLayout->addWidget(m_sidePanel, 0);
    mainLayout->addWidget(m_videoContainer, 1);

    return mainWidget;
}

void LiveVideoPlugin::updateLayout()
{
    static QMap<int, QList<QPair<int,int>>> layouts = {
        {1, {{0,0}}},
        {4, {{0,0},{0,1},{1,0},{1,1}}},
        {6, {{0,0},{0,1},{0,2},{1,0},{1,1},{1,2}}},
        {8, {{0,0},{0,1},{0,2},{0,3},{1,0},{1,1},{1,2},{1,3}}},
        {9, {{0,0},{0,1},{0,2},{1,0},{1,1},{1,2},{2,0},{2,1},{2,2}}},
        {16, {{0,0},{0,1},{0,2},{0,3},{1,0},{1,1},{1,2},{1,3},{2,0},{2,1},{2,2},{2,3},{3,0},{3,1},{3,2},{3,3}}}
    };

    QList<QPair<int,int>> positions = layouts.value(m_currentLayout);

    QStringList activeCells;
    for (const auto& pos : positions) {
        int row = pos.first;
        int col = pos.second;
        int index = row * 4 + col;
        QString cellId = QString("cell-%1").arg(index);
        activeCells.append(cellId);
    }

    QGridLayout* gridLayout = qobject_cast<QGridLayout*>(m_cells["cell-0"]->parentWidget()->layout());

    for (auto it = m_cells.begin(); it != m_cells.end(); ++it) {
        QString cellId = it.key();
        VideoCell* cell = it.value();

        if (activeCells.contains(cellId)) {
            cell->show();
            int index = activeCells.indexOf(cellId);
            int row = index / 4;
            int col = index % 4;
            gridLayout->addWidget(cell, row, col);
        } else {
            cell->hide();
        }
    }
}

VideoCell* LiveVideoPlugin::getCell(const QString& cellId)
{
    return m_cells.value(cellId);
}

void LiveVideoPlugin::setLayout(int cells)
{
    m_currentLayout = cells;
    updateLayout();
}

void LiveVideoPlugin::playStream(const QString& cellId, const QString& url, const QString& title)
{
    VideoCell* cell = getCell(cellId);
    if (cell) {
        cell->setTitle(title);
        cell->setPlaying(true);
        LOG_INFO("LiveVideoPlugin", QString("Playing: %1 -> %2").arg(cellId).arg(url));
        emit streamStarted(cellId);
    }
}

void LiveVideoPlugin::stopStream(const QString& cellId)
{
    VideoCell* cell = getCell(cellId);
    if (cell) {
        cell->setPlaying(false);
        LOG_INFO("LiveVideoPlugin", QString("Stopped: %1").arg(cellId));
        emit streamStopped(cellId);
    }
}

void LiveVideoPlugin::onStreamRequested(const QString& topic, EventDataPtr)
{
    Q_UNUSED(topic)
}

void LiveVideoPlugin::onStreamStop(const QString& topic, EventDataPtr)
{
    Q_UNUSED(topic)
}

QWidget* LiveVideoPlugin::createSidePanel(QWidget* parent)
{
    m_sidePanel = new QWidget(parent);
    m_sidePanel->setObjectName("VideoSidePanel");

    QVBoxLayout* layout = new QVBoxLayout(m_sidePanel);
    layout->setContentsMargins(0, 0, 0, 0);

    m_tabWidget = new QTabWidget(m_sidePanel);

    m_regionTree = new DeviceTree(m_tabWidget);
    QObject::connect(m_regionTree, QOverload<QVariantMap>::of(&DeviceTree::itemDoubleClicked),
                     this, &LiveVideoPlugin::onDeviceTreeItemClicked);
    QObject::connect(m_regionTree, QOverload<const QString&>::of(&DeviceTree::nodeNotFound), [](const QString& key) {
        LOG_WARNING("LiveVideoPlugin", QString("Region node not found: %1").arg(key));
    });
    m_tabWidget->addTab(m_regionTree, tr("地区"));

    m_groupTree = new DeviceTree(m_tabWidget);
    QObject::connect(m_groupTree, QOverload<QVariantMap>::of(&DeviceTree::itemDoubleClicked),
                     this, &LiveVideoPlugin::onDeviceTreeItemClicked);
    QObject::connect(m_groupTree, QOverload<const QString&>::of(&DeviceTree::nodeNotFound), [](const QString& key) {
        LOG_WARNING("LiveVideoPlugin", QString("Group node not found: %1").arg(key));
    });
    m_tabWidget->addTab(m_groupTree, tr("自定义分组"));

    loadRegionTree();
    loadGroupTree();

    layout->addWidget(m_tabWidget);

    return m_sidePanel;
}

void LiveVideoPlugin::onChannelClicked(int deviceId, int channelId, const QString& channelName)
{
    Q_UNUSED(channelId)
    QString url = QString("rtsp://localhost/live/%1").arg(deviceId);
    QString cellId;

    for (auto cell : m_cells.values()) {
        if (!cell->isPlaying()) {
            cellId = cell->cellId();
            break;
        }
    }

    if (cellId.isEmpty()) {
        cellId = "cell-0";
    }

    playStream(cellId, url, channelName);
}

void LiveVideoPlugin::loadRegionTree()
{
    QFuture<QPair<bool, QJsonObject>> future = QtConcurrent::run([]() {
        QJsonObject response;
        QString error;
        bool success = ServerApi::instance()->getSync("/api/v1/regions", response, error);
        Q_UNUSED(error)
        return QPair<bool, QJsonObject>(success, response);
    });

    m_regionTreeWatcher = new QFutureWatcher<QPair<bool, QJsonObject>>(this);
    QObject::connect(m_regionTreeWatcher, &QFutureWatcher<QPair<bool, QJsonObject>>::finished,
            this, &LiveVideoPlugin::onRegionTreeFinished);
    m_regionTreeWatcher->setFuture(future);
}

void LiveVideoPlugin::loadGroupTree()
{
    QFuture<QPair<bool, QJsonObject>> future = QtConcurrent::run([]() {
        QJsonObject response;
        QString error;
        bool success = ServerApi::instance()->getSync("/api/v1/groups", response, error);
        Q_UNUSED(error)
        return QPair<bool, QJsonObject>(success, response);
    });

    m_groupTreeWatcher = new QFutureWatcher<QPair<bool, QJsonObject>>(this);
    QObject::connect(m_groupTreeWatcher, &QFutureWatcher<QPair<bool, QJsonObject>>::finished,
            this, &LiveVideoPlugin::onGroupTreeFinished);
    m_groupTreeWatcher->setFuture(future);
}

void LiveVideoPlugin::onRegionTreeFinished()
{
    QPair<bool, QJsonObject> result = m_regionTreeWatcher->future().result();
    bool success = result.first;
    const QJsonObject& response = result.second;

    if (!success) {
        LOG_ERROR("LiveVideoPlugin", "Failed to load region tree");
        return;
    }

    QList<DeviceTreeNode> nodes;
    QJsonArray data = response.value("data").toArray();

    for (const QJsonValue& value : data) {
        DeviceTreeNode node;
        node.key = value.toObject()["id"].toString();
        node.name = value.toObject()["name"].toString();
        node.type = "region";
        nodes.append(node);
    }

    m_regionTree->setNodes(nodes);
}

void LiveVideoPlugin::onGroupTreeFinished()
{
    QPair<bool, QJsonObject> result = m_groupTreeWatcher->future().result();
    bool success = result.first;
    const QJsonObject& response = result.second;

    if (!success) {
        LOG_ERROR("LiveVideoPlugin", "Failed to load group tree");
        return;
    }

    QList<DeviceTreeNode> nodes;
    QJsonArray data = response.value("data").toArray();

    for (const QJsonValue& value : data) {
        DeviceTreeNode node;
        node.key = value.toObject()["id"].toString();
        node.name = value.toObject()["name"].toString();
        node.type = "group";
        nodes.append(node);
    }

    m_groupTree->setNodes(nodes);
}

void LiveVideoPlugin::onDeviceTreeItemClicked(const QVariantMap& meta)
{
    QString type = meta["type"].toString();
    if (type == "region" || type == "group" || type == "device") {
        QString key = meta["key"].toString();
        int deviceId = key.toInt();
        if (deviceId > 0) {
            if (m_videoContainer) {
                QString url = QString("rtsp://localhost/live/%1").arg(deviceId);
                m_videoContainer->playStream(QString(), url, meta["name"].toString());
            } else {
                onChannelClicked(deviceId, 0, meta["name"].toString());
            }
        }
    }
}

}
