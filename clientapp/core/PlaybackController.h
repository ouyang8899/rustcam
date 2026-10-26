#pragma once

#include <QObject>
#include <QList>
#include <QMutex>
#include <functional>
#include "StreamSource.h"

namespace rustcam {

class RUSTCAM_CORE_API PlaybackGroup : public QObject {
    Q_OBJECT
public:
    explicit PlaybackGroup(int groupId, QObject* parent = nullptr);
    ~PlaybackGroup();

    int groupId() const { return m_groupId; }
    const QList<StreamSource*>& cells() const { return m_cells; }

    void addCell(StreamSource* cell);
    void removeCell(StreamSource* cell);
    void clearCells();

    void syncTo(int64_t baseTimeMs);
    void setBaseTime(int64_t baseTimeMs) { m_baseTimeMs = baseTimeMs; }
    int64_t baseTimeMs() const { return m_baseTimeMs; }
    /// 组内所有窗口定位到指定时间（不论播放/暂停态），越界时停播
    void seekTo(int64_t baseTimeMs);
    void play();
    void pause();
    void stop();

signals:
    void cellAdded(StreamSource* cell);
    void cellRemoved(StreamSource* cell);

private:
    int m_groupId;
    QList<StreamSource*> m_cells;
    int64_t m_baseTimeMs = 0;
};

class RUSTCAM_CORE_API PlaybackController : public QObject {
    Q_OBJECT
public:
    enum class SyncState {
        Idle,
        Syncing,
        Synced
    };

    explicit PlaybackController(QObject* parent = nullptr);
    ~PlaybackController();

    int64_t baseTimeMs() const { return m_baseTimeMs; }
    const QList<PlaybackGroup*>& groups() const { return m_groups; }
    PlaybackGroup* defaultGroup() { return m_defaultGroup; }

    SyncState syncState() const { return m_syncState; }

    void setBaseTime(int64_t timeMs);
    /// 只同步指定通道的播放组（各通道播放位置互相独立）
    void setGroupBaseTime(PlaybackGroup* group, int64_t timeMs);
    int64_t groupBaseTime(PlaybackGroup* group) const;
    void addGroup(PlaybackGroup* group);
    void removeGroup(PlaybackGroup* group);
    void removeGroup(int groupId);

    void play();
    void pause();
    void stop();
    void setSpeed(double speed);

    PlaybackGroup* createGroup();
    void destroyGroup(int groupId);

signals:
    void baseTimeChanged(int64_t timeMs);
    void groupAdded(PlaybackGroup* group);
    void groupRemoved(int groupId);
    void syncStateChanged(SyncState state);

private slots:
    void onBatchSync();

private:
    int64_t m_baseTimeMs = 0;
    QList<PlaybackGroup*> m_groups;
    PlaybackGroup* m_defaultGroup;
    int m_nextGroupId = 1;
    QMutex m_mutex;
    SyncState m_syncState = SyncState::Idle;
    int m_batchStartIndex = 0;
    int64_t m_batchTimeMs = 0;
    static constexpr int BATCH_SIZE = 4;
};

}
