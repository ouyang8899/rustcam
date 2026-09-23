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

    EventBus::instance()->subscribe("video/stream/request", this, SLOT(onStreamRequested(QString,QVariantMap)));
    EventBus::instance()->subscribe("video/stream/stop", this, SLOT(onStreamStop(QString,QVariantMap)));

    LOG_INFO("LiveVideoPlugin", "Initialized");
    return true;
}

void LiveVideoPlugin::shutdown()
{
    EventBus::instance()->unsubscribeAll(this);
}

QWidget* LiveVideoPlugin::createWorkspace(QWidget* parent)
{
    m_workspace = new QWidget(parent);
    m_workspace->setObjectName("workspaceArea");
    m_workspace->setStyleSheet(R"(
        QWidget#workspaceArea {
            background-color: #2b2b2b;
        }
    )");

    QVBoxLayout* layout = new QVBoxLayout(m_workspace);
    layout->setContentsMargins(8, 8, 8, 8);
    layout->setSpacing(8);

    QToolBar* toolbar = new QToolBar(m_workspace);
    toolbar->setObjectName("VideoToolbar");
    toolbar->setMovable(false);

    QLabel* layoutLabel = new QLabel(tr("Layout:"));
    layoutLabel->setObjectName("ToolbarLabel");
    toolbar->addWidget(layoutLabel);

    QComboBox* layoutCombo = new QComboBox();
    layoutCombo->setObjectName("LayoutCombo");
    layoutCombo->addItems({"1", "4", "6", "8", "9", "13", "16"});
    layoutCombo->setCurrentText("4");
    QObject::connect(layoutCombo, &QComboBox::currentTextChanged, this, [this](const QString& text) {
        setLayout(text.toInt());
    });
    toolbar->addWidget(layoutCombo);

    toolbar->addSeparator();

    QAction* stopAllAction = new QAction(tr("Stop All"), this);
    stopAllAction->setObjectName("StopAllAction");
    QObject::connect(stopAllAction, &QAction::triggered, this, [this]() {
        for (auto cell : m_cells.values()) {
            stopStream(cell->cellId());
        }
    });
    toolbar->addAction(stopAllAction);

    layout->addWidget(toolbar);

    QWidget* gridContainer = new QWidget(m_workspace);
    QGridLayout* gridLayout = new QGridLayout(gridContainer);
    gridLayout->setContentsMargins(0, 0, 0, 0);
    gridLayout->setSpacing(8);

    for (int i = 0; i < 16; ++i) {
        QString cellId = QString("cell-%1").arg(i);
        VideoCell* cell = new VideoCell(cellId, gridContainer);
        m_cells[cellId] = cell;
        cell->hide();
    }

    updateLayout();
    layout->addWidget(gridContainer, 1);

    return m_workspace;
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

void LiveVideoPlugin::onStreamRequested(const QString& topic, const QVariantMap& payload)
{
    Q_UNUSED(topic)
    QString cellId = payload["cellId"].toString();
    QString url = payload["url"].toString();
    QString title = payload["title"].toString();

    if (cellId.isEmpty()) {
        for (auto cell : m_cells.values()) {
            if (!cell->isPlaying()) {
                cellId = cell->cellId();
                break;
            }
        }
    }

    playStream(cellId, url, title);
}

void LiveVideoPlugin::onStreamStop(const QString& topic, const QVariantMap& payload)
{
    Q_UNUSED(topic)
    QString cellId = payload["cellId"].toString();
    if (!cellId.isEmpty()) {
        stopStream(cellId);
    }
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
            onChannelClicked(deviceId, 0, meta["name"].toString());
        }
    }
}

}
