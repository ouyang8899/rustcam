#pragma once

#include "TourScheme.h"
#include "TourTypes.h"

#include <QElapsedTimer>
#include <QObject>
#include <QSet>
#include <QStringList>
#include <QTimer>
#include <QVector>

class QDateTime;

namespace rustcam {

struct FillResult;

/// 轮询调度器：单线程串行状态机。
///
/// 负责批次划分、游标推进、dwell/超时计时、cycle 计数、补位、暂停恢复与翻页；
/// 只通过信号驱动播放器，不直接触碰 UI。
class TourScheduler : public QObject {
    Q_OBJECT
public:
    /// 窗口与通道的绑定。
    struct SlotAssignment {
        QString layoutItemId;
        TourChannelRef channel;
    };

    /// 一个批次的运行态。
    struct Batch {
        int index = 0;
        QVector<SlotAssignment> assignments;
        QSet<QString> keys;
        quint64 generation = 0;
        int backfillUsed = 0;
        bool displayable = false;
        bool finished = false;      ///< 批次是否已结束（结束后的迟到回调不再生效）
        bool replay = false;        ///< 是否为历史批次回放（回放结束不再入历史）

        qint64 batchDeadline = 0;   ///< 首帧等待截止（单调时钟）
        qint64 dwellDeadline = 0;   ///< 可展示后的 dwell 截止（单调时钟）
    };

    explicit TourScheduler(QObject* parent = nullptr);
    ~TourScheduler() override;

    void start(const TourScheme& scheme, const TourSnapshot& snapshot,
               const QStringList& lockedItemIds);
    void pause();
    void resume();
    void stop();

    /// 翻到下一批；已到队尾时发出 pageAtEnd。
    void nextBatch();
    /// 翻到上一批；已在队首时发出 pageAtEnd。
    void prevBatch();
    /// 重试某个窗口当前的通道。
    void retrySlot(const QString& layoutItemId);
    /// 全屏时暂停换批与补位，退出全屏后恢复。
    void setFullscreenPaused(bool paused);

    void onFirstFrame(const QString& layoutItemId, quint64 generation);
    void onPlayFailed(const QString& layoutItemId, quint64 generation, const QString& reason);
    void onDisconnected(const QString& layoutItemId, quint64 generation);
    void onStalled(const QString& layoutItemId, quint64 generation);

    /// 判断给定时间是否落在运行时段内（便于单测覆盖跨天/星期边界）。
    static bool inRunWindowAt(const RunWindow& window, const QDateTime& now);

    int currentBatchIndex() const { return m_batch.index; }
    TourTaskState taskState() const { return m_state; }
    int cycle() const { return m_cycle; }
    int availableSlots() const { return m_s; }
    int queueSize() const { return m_n; }
    /// 当前批次中该通道所在的窗口；不在当前批次时返回空串。
    QString layoutItemIdForChannel(const TourChannelRef& channel) const;

signals:
    void playRequested(const QString& layoutItemId, TourChannelRef channel, quint64 generation);
    /// layoutItemId 为 "*" 时表示释放全部。
    void releaseRequested(const QString& layoutItemId);
    void windowStateChanged(const QString& layoutItemId, TourChannelState state,
                            TourChannelRef channel, QString reason);
    void batchStarted(int batchIndex, int cycle, QVector<TourChannelRef> channels);
    void batchFinished(int batchIndex, int cycle);
    void taskStateChanged(int state);
    void countdown(int dwellRemainMs, int batchTimeoutRemainMs);
    void cycleAdvanced(int cycle);
    void pageAtEnd();

private slots:
    void onTick();

private:
    qint64 nowMs() const { return m_clock.elapsed(); }
    void setState(TourTaskState state);
    /// 由填充结果构造批次（分配窗口、递增 generation、设置首帧截止时间）。
    Batch makeBatch(int index, const FillResult& plan);
    /// 发布当前批次：逐槽下发状态/播放请求、通知批次开始并启动计时。
    QVector<TourChannelRef> activateBatch();
    void beginBatch();
    void replayBatch(const Batch& batch);
    void finishBatch();
    void backfillSlot(const QString& layoutItemId);
    int slotIndexOf(const QString& layoutItemId) const;
    bool inRunWindow() const;
    bool windowExpired() const;

    TourScheme m_scheme;
    TourSnapshot m_snapshot;
    QStringList m_lockedItems;
    QStringList m_unlockedItems;

    int m_n = 0;   ///< 参与轮询的通道数
    int m_s = 0;   ///< 可用窗口数
    int m_cursor = 0;
    int m_cycle = 0;

    QSet<QString> m_covered;
    QSet<QString> m_failed;

    Batch m_batch;
    QVector<Batch> m_batchHistory;
    quint64 m_generation = 0;

    TourTaskState m_state = TourTaskState::Idle;
    QTimer m_tick;
    QElapsedTimer m_clock;
    qint64 m_gapDeadline = 0;
    qint64 m_frozenDwell = -1;
    qint64 m_frozenTimeout = -1;
    bool m_fullscreenPaused = false;
};

} // namespace rustcam
