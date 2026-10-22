#pragma once

#include <QObject>
#include <QMap>
#include <atomic>
#include "CoreExport.h"
#include "PlayerLayout.h"

namespace rustcam {

class VideoPlayer;

class RUSTCAM_CORE_API VideoWidgetManager : public QObject {
    Q_OBJECT
public:
    static VideoWidgetManager& instance();

    void applyLayout(const PlayerLayout& layout, QWidget* parent);
    void clearLayout();

    quint64 allocateTarget(QWidget* widget);
    void releaseTarget(quint64 targetId);

    void registerPlayer(const QString& itemId, VideoPlayer* player);
    void unregisterPlayer(const QString& itemId);

    void play(const QString& itemId, const QString& url);
    void stop(const QString& itemId);
    void stopAll();

    void switchTarget(const QString& itemId, quint64 targetId);

    VideoPlayer* getPlayer(const QString& itemId);
    QWidget* getTarget(quint64 targetId);
    QList<VideoPlayer*> allPlayers() const;
    QString getItemId(VideoPlayer* player) const;
    QStringList getPlayerIds() const;

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
    std::atomic<quint64> m_nextTargetId{1};
    PlayerLayout m_currentLayout;
};

}
