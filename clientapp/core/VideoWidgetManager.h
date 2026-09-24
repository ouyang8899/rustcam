#pragma once

#include <QObject>
#include <QMap>
#include <QMutex>
#include <atomic>
#include "CoreExport.h"
#include "PlayerLayout.h"

namespace rustcam {

class VideoPlayer;

class RUSTCAM_CORE_API VideoWidgetManager : public QObject {
    Q_OBJECT
public:
    static VideoWidgetManager& instance();

    // 布局应用
    void applyLayout(const PlayerLayout& layout, QWidget* parent);
    void clearLayout();

    // 渲染目标
    quint64 allocateTarget(QWidget* widget);
    void releaseTarget(quint64 targetId);

    // player 注册
    void registerPlayer(const QString& itemId, VideoPlayer* player);
    void unregisterPlayer(const QString& itemId);

    // 播放控制
    void play(const QString& itemId, const QString& url);
    void stop(const QString& itemId);
    void stopAll();

    // 切换渲染目标（layout 切换时）
    void switchTarget(const QString& itemId, quint64 targetId);

    // 获取
    VideoPlayer* getPlayer(const QString& itemId);
    QWidget* getTarget(quint64 targetId);
    QList<VideoPlayer*> allPlayers() const;
    QString getItemId(VideoPlayer* player) const;

signals:
    void layoutChanged(const PlayerLayout& layout);
    void streamStarted(const QString& itemId);
    void streamStopped(const QString& itemId);
    void streamError(const QString& itemId, const QString& error);

private:
    explicit VideoWidgetManager(QObject* parent = nullptr);
    ~VideoWidgetManager();
    Q_DISABLE_COPY(VideoWidgetManager)

    QMap<QString, VideoPlayer*> m_players;
    QMap<quint64, QWidget*> m_renderTargets;
    QMap<QString, quint64> m_itemTargetMap;
    QMap<VideoPlayer*, QString> m_playerItemMap;
    mutable QMutex m_mutex;
    std::atomic<quint64> m_nextTargetId{1};
    PlayerLayout m_currentLayout;
};

}
