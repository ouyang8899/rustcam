#include "VideoWidgetManager.h"
#include "VideoPlayer.h"
#include <QDebug>

namespace rustcam {

VideoWidgetManager::VideoWidgetManager(QObject* parent)
    : QObject(parent)
{
}

VideoWidgetManager::~VideoWidgetManager() {
    stopAll();
}

VideoWidgetManager& VideoWidgetManager::instance() {
    static VideoWidgetManager inst;
    return inst;
}

quint64 VideoWidgetManager::allocateTarget(QWidget* widget) {
    quint64 id = m_nextTargetId.fetch_add(1);
    m_renderTargets[id] = widget;
    return id;
}

void VideoWidgetManager::releaseTarget(quint64 targetId) {
    m_renderTargets.remove(targetId);
}

void VideoWidgetManager::registerPlayer(const QString& itemId, VideoPlayer* player) {
    m_players[itemId] = player;
    m_playerItemMap[player] = itemId;
}

void VideoWidgetManager::unregisterPlayer(const QString& itemId) {
    if (m_players.contains(itemId)) {
        VideoPlayer* player = m_players.take(itemId);
        m_playerItemMap.remove(player);
        m_itemTargetMap.remove(itemId);
    }
}

void VideoWidgetManager::play(const QString& itemId, const QString& url) {
    VideoPlayer* player = m_players.value(itemId);
    if (!player) {
        qWarning() << "VideoWidgetManager::play - no player for itemId:" << itemId;
        return;
    }

    player->play(url);
}

void VideoWidgetManager::stop(const QString& itemId) {
    VideoPlayer* player = m_players.value(itemId);
    if (player) {
        player->stop();
    }
}

void VideoWidgetManager::stopAll() {
    for (auto* player : m_players.values()) {
        if (player) {
            player->stop();
        }
    }
}

//void VideoWidgetManager::switchTarget(const QString& itemId, quint64 targetId) {
//    VideoPlayer* player = m_players.value(itemId);
//    if (player) {
//        player->switchTarget(targetId);
//    }
//}

void VideoWidgetManager::applyLayout(const PlayerLayout& layout, QWidget* parent) {
    Q_UNUSED(parent);
    m_currentLayout = layout;

    for (auto* player : m_players.values()) {
        if (player) {
            quint64 targetId = allocateTarget(player);
            m_itemTargetMap[m_playerItemMap[player]] = targetId;
         //   player->switchTarget(targetId);
        }
    }

    emit layoutChanged(layout);
}

void VideoWidgetManager::clearLayout() {
    for (const QString& itemId : m_players.keys()) {
        VideoPlayer* player = m_players.take(itemId);
        m_playerItemMap.remove(player);
        m_itemTargetMap.remove(itemId);
    }
    m_players.clear();
    m_renderTargets.clear();
    m_itemTargetMap.clear();
    m_playerItemMap.clear();
}

VideoPlayer* VideoWidgetManager::getPlayer(const QString& itemId) {
    return m_players.value(itemId, nullptr);
}

QWidget* VideoWidgetManager::getTarget(quint64 targetId) {
    return m_renderTargets.value(targetId, nullptr);
}

QList<VideoPlayer*> VideoWidgetManager::allPlayers() const {
    return m_players.values();
}

QString VideoWidgetManager::getItemId(VideoPlayer* player) const {
    return m_playerItemMap.value(player, QString());
}

QStringList VideoWidgetManager::getPlayerIds() const {
    return m_players.keys();
}

}
