#include "VideoPlayerContainer.h"
#include "NavigationBar.h"
#include "core/VideoPlayer.h"
#include "core/EventBus.h"
#include "common/Logger.h"
#include <QVBoxLayout>
#include <QDebug>
#include <QtConcurrent>
#include <QFutureWatcher>

namespace rustcam {

VideoPlayerContainer::VideoPlayerContainer(QWidget* parent)
    : QWidget(parent)
    , m_videoArea(nullptr)
    , m_navBar(nullptr)
{
    setAcceptDrops(true);

    QVBoxLayout* layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    m_videoArea = new QWidget(this);
    m_videoArea->setObjectName("videoArea");
    // 容器统一接收拖放，避免视频区拦截设备拖入
    m_videoArea->setAcceptDrops(false);

    m_navBar = new NavigationBar(this);
    m_navBar->setObjectName("navBar");

    layout->addWidget(m_videoArea, 1);
    layout->addWidget(m_navBar, 0);

    connect(m_navBar, &NavigationBar::layoutSelected, this, &VideoPlayerContainer::onLayoutSelected);

}

VideoPlayerContainer::~VideoPlayerContainer() {
    VideoWidgetManager::instance().clearLayout();
}

void VideoPlayerContainer::initialize()
{
    if (DataManager::instance()->isLayoutsLoaded()) {
        onLayoutsReady();
    } else {
        connect(DataManager::instance(), &DataManager::layoutsLoaded, this, &VideoPlayerContainer::onLayoutsReady, Qt::UniqueConnection);
        DataManager::instance()->loadLayouts();
    }
}

void VideoPlayerContainer::onLayoutsReady() {
    disconnect(DataManager::instance(), &DataManager::layoutsLoaded, this, &VideoPlayerContainer::onLayoutsReady);
    m_layouts = DataManager::instance()->layouts();

    m_navBar->setLayouts(m_layouts);

    if (!m_layouts.isEmpty()) {
        m_currentLayout = m_layouts.first();
        calculateVideoLayout(m_currentLayout);
    }
}

void VideoPlayerContainer::onLayoutSelected(int index) {
    if (index < 0 || index >= m_layouts.size()) return;

    m_selectedItemId.clear();
    m_currentLayout = m_layouts[index];
    calculateVideoLayout(m_currentLayout);
}

void VideoPlayerContainer::calculateVideoLayout(const PlayerLayout& layout) {
    const int margin = 2;
    const int spacing = 2;

    int availableW = m_videoArea->width();
    int availableH = m_videoArea->height();

    if (availableW <= 0 || availableH <= 0 || layout.cols <= 0 || layout.rows <= 0) {
        return;
    }

    int cw = (availableW - margin * 2 - spacing * (layout.cols - 1)) / layout.cols;
    int ch = (availableH - margin * 2 - spacing * (layout.rows - 1)) / layout.rows;

    QSet<QString> currentItemIds;
    for (const auto& item : layout.layoutJson) {
        currentItemIds.insert(item.id);

        int x = margin + item.col * (cw + spacing);
        int y = margin + item.row * (ch + spacing);
        int w = item.colSpan * cw + (item.colSpan - 1) * spacing;
        int h = item.rowSpan * ch + (item.rowSpan - 1) * spacing;

        VideoPlayer* player = VideoWidgetManager::instance().getPlayer(item.id);
        QWidget* wrapper = nullptr;

        if (!player) {
            wrapper = new QWidget(m_videoArea);
            wrapper->setObjectName("VideoPlayerWrapper");
            player = new VideoPlayer(wrapper);
            player->setContentsMargins(1, 1, 1, 1);
            VideoWidgetManager::instance().registerPlayer(item.id, player);
            m_playerWrappers[item.id] = wrapper;
            // 播放器由容器拥有：连接在这里一次建立，之后不再变动
            // 播放器的身份用 uid 传递，换格子后依然能定位到当前所在的位置
            connect(player, &VideoPlayer::clicked, this, &VideoPlayerContainer::onPlayerClicked);
            connect(player, &VideoPlayer::playerDropped, this, &VideoPlayerContainer::onPlayerDropped);
            connect(player, &VideoPlayer::deviceDropped, this, &VideoPlayerContainer::onDeviceDropped);
        } else {
            wrapper = m_playerWrappers.value(item.id);
        }

        const int borderWidth = 1;
        wrapper->setGeometry(x, y, w, h);
        player->setGeometry(borderWidth, borderWidth, w - borderWidth * 2, h - borderWidth * 2);
        player->setSelected(item.id == m_selectedItemId);

        bool isSelected = (item.id == m_selectedItemId);
        wrapper->setStyleSheet(isSelected
            ? "QWidget#VideoPlayerWrapper { border: 2px solid #0078D4; background-color: #1a1a1a; }"
            : "QWidget#VideoPlayerWrapper { border: 2px solid #1a1a1a; background-color: #1a1a1a; }");

        wrapper->show();
        player->show();
    }

    for (const QString& itemId : VideoWidgetManager::instance().getPlayerIds()) {
        if (!currentItemIds.contains(itemId)) {
            if (QWidget* wrapper = m_playerWrappers.take(itemId)) {
                wrapper->deleteLater();
            }
            VideoWidgetManager::instance().unregisterPlayer(itemId);
        }
    }
}

void VideoPlayerContainer::onVideoPlayerClicked(const QString& itemId) {
    if (itemId.isEmpty()) {
        return;
    }
    QString oldSelected = m_selectedItemId;
    m_selectedItemId = itemId;

    if (!oldSelected.isEmpty()) {
        VideoPlayer* oldPlayer = VideoWidgetManager::instance().getPlayer(oldSelected);
        if (oldPlayer) oldPlayer->setSelected(false);
        if (QWidget* oldWrapper = m_playerWrappers.value(oldSelected)) {
            oldWrapper->setStyleSheet("#VideoPlayerWrapper { border: none; }");
        }
    }

    VideoPlayer* newPlayer = VideoWidgetManager::instance().getPlayer(itemId);
    if (newPlayer) newPlayer->setSelected(true);
    if (QWidget* newWrapper = m_playerWrappers.value(itemId)) {
        newWrapper->setStyleSheet("#VideoPlayerWrapper { border: 2px solid #0078D4; }");
    }

    EventDataPtr eventData = EventData::create();
    eventData->m_payload["itemId"] = itemId;
    EventBus::instance()->publish(TOPIC_VIDEO_CELL_SELECTED, eventData);
}

// ── 播放器在格子之间拖动换位 ────────────────────────────────────────

QString VideoPlayerContainer::itemIdOfPlayer(VideoPlayer* player) const {
    for (const QString& itemId : VideoWidgetManager::instance().getPlayerIds()) {
        if (VideoWidgetManager::instance().getPlayer(itemId) == player) {
            return itemId;
        }
    }
    return QString();
}

QString VideoPlayerContainer::itemIdOfUid(const QString& uid) const {
    for (const QString& itemId : VideoWidgetManager::instance().getPlayerIds()) {
        VideoPlayer* player = VideoWidgetManager::instance().getPlayer(itemId);
        if (player && player->uid() == uid) {
            return itemId;
        }
    }
    return QString();
}

VideoPlayer* VideoPlayerContainer::findPlayerByUid(const QString& uid) const {
    for (const QString& itemId : VideoWidgetManager::instance().getPlayerIds()) {
        VideoPlayer* player = VideoWidgetManager::instance().getPlayer(itemId);
        if (player && player->uid() == uid) {
            return player;
        }
    }
    return nullptr;
}

void VideoPlayerContainer::layoutPlayerInWrapper(VideoPlayer* player, QWidget* wrapper) {
    if (!player || !wrapper) {
        return;
    }
    const int borderWidth = 1;
    // 只摆位置，不改父窗口（窗口句柄不变）
    player->setGeometry(borderWidth, borderWidth,
                        wrapper->width() - borderWidth * 2,
                        wrapper->height() - borderWidth * 2);
}

void VideoPlayerContainer::applySelectionStyles() {
    for (const QString& itemId : VideoWidgetManager::instance().getPlayerIds()) {
        VideoPlayer* player = VideoWidgetManager::instance().getPlayer(itemId);
        if (player) {
            player->setSelected(itemId == m_selectedItemId);
        }
        QWidget* wrapper = m_playerWrappers.value(itemId);
        if (!wrapper) {
            continue;
        }
        wrapper->setStyleSheet(itemId == m_selectedItemId
            ? "QWidget#VideoPlayerWrapper { border: 2px solid #0078D4; background-color: #1a1a1a; }"
            : "QWidget#VideoPlayerWrapper { border: 2px solid #1a1a1a; background-color: #1a1a1a; }");
    }
}

void VideoPlayerContainer::onPlayerClicked(const QString& uid) {
    // 多个播放器共用这一个槽：按 uid 定位到当前所在的格子
    onVideoPlayerClicked(itemIdOfUid(uid));
}

void VideoPlayerContainer::onDeviceDropped(const QString& uid, const QVariantMap& meta) {
    handleDrop(meta, itemIdOfUid(uid));
}

void VideoPlayerContainer::onPlayerDropped(const QString& targetUid, const QString& sourceUid) {
    VideoPlayer* target = findPlayerByUid(targetUid);
    VideoPlayer* source = findPlayerByUid(sourceUid);
    if (!source || !target || source == target) {
        return;
    }

    const QString sourceItemId = itemIdOfPlayer(source);
    const QString targetItemId = itemIdOfPlayer(target);
    if (sourceItemId.isEmpty() || targetItemId.isEmpty()) {
        return;
    }

    QWidget* sourceWrapper = m_playerWrappers.value(sourceItemId);
    QWidget* targetWrapper = m_playerWrappers.value(targetItemId);
    if (!sourceWrapper || !targetWrapper) {
        return;
    }

    LOG_INFO("VideoPlayerContainer", QString("swap players: %1 <-> %2").arg(sourceItemId, targetItemId));

    // 两个位置上的流也要跟着换
    const QString sourceUrl = m_playingUrls.value(sourceItemId);
    const QString targetUrl = m_playingUrls.value(targetItemId);

    // 交换两个格子的位置：播放器留在原来的父窗口里（父窗口不变 → 窗口句柄不变 → 画面不用重建）
    const QRect sourceRect = sourceWrapper->geometry();
    const QRect targetRect = targetWrapper->geometry();
    sourceWrapper->setGeometry(targetRect);
    targetWrapper->setGeometry(sourceRect);
    layoutPlayerInWrapper(source, sourceWrapper);
    layoutPlayerInWrapper(target, targetWrapper);

    // 格子标识与流都跟着位置走：itemId 始终代表那个位置上的播放器
    m_playerWrappers.insert(targetItemId, sourceWrapper);
    m_playerWrappers.insert(sourceItemId, targetWrapper);
    VideoWidgetManager::instance().registerPlayer(targetItemId, source);
    VideoWidgetManager::instance().registerPlayer(sourceItemId, target);
    m_playingUrls.insert(targetItemId, sourceUrl);
    m_playingUrls.insert(sourceItemId, targetUrl);

    applySelectionStyles();
}

void VideoPlayerContainer::onStreamRequested(const QString& topic) {
    Q_UNUSED(topic)
}

void VideoPlayerContainer::playStream(const QString& itemId, const QString& url, const QString& title) {
    QString targetId = itemId;

    if (targetId.isEmpty()) {
        targetId = selectTargetItem();
    }

    LOG_INFO("VideoPlayerContainer", QString("playStream: targetId=%1, url=%2, title=%3").arg(targetId).arg(url).arg(title));

    if (targetId.isEmpty()) {
        LOG_ERROR("VideoPlayerContainer", "playStream: targetId is empty!");
        return;
    }

    VideoPlayer* player = VideoWidgetManager::instance().getPlayer(targetId);
    if (player) {
        m_playingUrls[targetId] = url;
        if (!title.isEmpty()) {
            player->setDeviceName(title);
        }
        player->play(url);
    } else {
        LOG_ERROR("VideoPlayerContainer", QString("playStream: player not found for targetId=%1, available players: %2")
            .arg(targetId)
            .arg(VideoWidgetManager::instance().getPlayerIds().join(", ")));
    }
}

QString VideoPlayerContainer::selectTargetItem() {
    LOG_INFO("VideoPlayerContainer", QString("selectTargetItem: m_selectedItemId=%1, layoutJson.size=%2")
        .arg(m_selectedItemId).arg(m_currentLayout.layoutJson.size()));
    if (!m_selectedItemId.isEmpty()) {
        VideoPlayer* selected = VideoWidgetManager::instance().getPlayer(m_selectedItemId);
        if (selected && !selected->isPlaying()) {
            LOG_INFO("VideoPlayerContainer", QString("selectTargetItem: returning selected item %1").arg(m_selectedItemId));
            return m_selectedItemId;
        }
    }

    for (const auto& item : m_currentLayout.layoutJson) {
        VideoPlayer* player = VideoWidgetManager::instance().getPlayer(item.id);
        if (player && !player->isPlaying()) {
            LOG_INFO("VideoPlayerContainer", QString("selectTargetItem: returning idle item %1").arg(item.id));
            return item.id;
        }
    }

    if (!m_currentLayout.layoutJson.isEmpty()) {
        QString firstId = m_currentLayout.layoutJson.first().id;
        LOG_INFO("VideoPlayerContainer", QString("selectTargetItem: returning first item %1").arg(firstId));
        return firstId;
    }

    LOG_ERROR("VideoPlayerContainer", "selectTargetItem: layoutJson is empty, no players available!");
    return QString();
}

void VideoPlayerContainer::dragEnterEvent(QDragEnterEvent* event) {
    if (event->mimeData()->hasFormat("application/x-rustcam-device")) {
        event->acceptProposedAction();
    }
}

void VideoPlayerContainer::dragMoveEvent(QDragMoveEvent* event) {
    if (event->mimeData()->hasFormat("application/x-rustcam-device")) {
        event->acceptProposedAction();
    }
}

void VideoPlayerContainer::dropEvent(QDropEvent* event) {
    if (!event->mimeData()->hasFormat("application/x-rustcam-device")) {
        return;
    }

    QByteArray data = event->mimeData()->data("application/x-rustcam-device");
    QJsonDocument doc = QJsonDocument::fromJson(data);
    QVariantMap meta = doc.object().toVariantMap();

    // 优先播到鼠标落点所在的格子；落在空隙/导航栏时退回当前选中格
    QString targetItemId;
    const QPoint pos = m_videoArea->mapFrom(this, event->pos());
    for (auto it = m_playerWrappers.constBegin(); it != m_playerWrappers.constEnd(); ++it) {
        if (it.value() && it.value()->geometry().contains(pos)) {
            targetItemId = it.key();
            break;
        }
    }

    handleDrop(meta, targetItemId);
    event->acceptProposedAction();
}

void VideoPlayerContainer::handleDrop(const QVariantMap& meta, const QString& targetItemId) {
    QString targetId = targetItemId;
    if (targetId.isEmpty() || !VideoWidgetManager::instance().getPlayer(targetId)) {
        targetId = selectTargetItem();
    }

    if (meta["type"].toString() != "channel") {
        return;
    }

    QString deviceTag = meta["device_tag"].toString();
    QString channelTag = meta["channel_tag"].toString();
    if (deviceTag.isEmpty() || channelTag.isEmpty()) {
        return;
    }

    QString title = meta["name"].toString();
    QtConcurrent::run([this, deviceTag, channelTag, title, targetId]() {
        // 播放地址统一由后端生成，避免客户端拼接 RTSP URL
        QString endpoint = QString("/api/v1/channels/%1/%2/play-links").arg(deviceTag, channelTag);
        QJsonObject response;
        QString error;
        bool success = ServerApi::instance()->getSync(endpoint, response, error);
        QMetaObject::invokeMethod(this, [this, success, response, error, title, targetId]() {
            onChannelPlayLinksReceived(success, response, error, title, targetId);
        }, Qt::QueuedConnection);
    });
}

void VideoPlayerContainer::onChannelPlayLinksReceived(bool success, const QJsonObject& response, const QString& error, const QString& title, const QString& targetId)
{
    if (!success) {
        qWarning() << "Failed to get play links:" << error;
        return;
    }
    QJsonObject data = response["data"].toObject();
    QString url = data["rtsp_media"].toString();
    if (url.isEmpty()) {
        url = data["hls"].toString();
    }
    if (url.isEmpty()) {
        url = data["flv"].toString();
    }
    if (url.isEmpty()) {
        qWarning() << "No playable URL found in play links response";
        return;
    }
    playStream(targetId, url, title);
}

void VideoPlayerContainer::resizeEvent(QResizeEvent* event) {
    QWidget::resizeEvent(event);
    if (!m_currentLayout.layoutJson.isEmpty() && m_videoArea && m_videoArea->width() > 0 && m_videoArea->height() > 0) {
        calculateVideoLayout(m_currentLayout);
    }
}

}
