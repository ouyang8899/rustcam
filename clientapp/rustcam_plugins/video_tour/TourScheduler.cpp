#include "TourScheduler.h"

#include <QDateTime>
#include <QTime>

#include <algorithm>

#include "TourAuditLog.h"
#include "TourBatchPlanner.h"

namespace rustcam {

namespace {

constexpr int kTickIntervalMs = 100;

QTime parseTime(const QString& text)
{
    const QTime time = QTime::fromString(text, QStringLiteral("HH:mm"));
    return time.isValid() ? time : QTime();
}

} // namespace

TourScheduler::TourScheduler(QObject* parent)
    : QObject(parent)
{
    m_tick.setInterval(kTickIntervalMs);
    connect(&m_tick, &QTimer::timeout, this, &TourScheduler::onTick);
}

TourScheduler::~TourScheduler() = default;

void TourScheduler::setState(TourTaskState state)
{
    if (m_state == state) {
        return;
    }
    m_state = state;
    emit taskStateChanged(static_cast<int>(state));
}

void TourScheduler::start(const TourScheme& scheme, const TourSnapshot& snapshot,
                          const QStringList& lockedItemIds)
{
    m_scheme = scheme;
    m_snapshot = snapshot;
    // 锁定窗口以 snapshot.lockedSet 为准；调用方未显式传入时也不会被轮换占用。
    m_lockedItems = lockedItemIds;
    for (const LockedSlot& slot : snapshot.lockedSet) {
        if (!m_lockedItems.contains(slot.layoutItemId)) {
            m_lockedItems.append(slot.layoutItemId);
        }
    }

    m_unlockedItems.clear();
    for (const QString& itemId : snapshot.layoutItemIds) {
        if (!m_lockedItems.contains(itemId)) {
            m_unlockedItems.append(itemId);
        }
    }

    m_n = snapshot.activeQueue.size();
    m_s = m_unlockedItems.size();

    m_cursor = 0;
    m_cycle = 0;
    m_covered.clear();
    m_failed.clear();
    m_batchHistory.clear();
    m_batch = Batch();
    m_generation = 1;
    m_gapDeadline = 0;
    m_frozenDwell = -1;
    m_frozenTimeout = -1;
    m_fullscreenPaused = false;

    m_clock.restart();

    if (m_s <= 0) {
        m_tick.stop();
        setState(TourTaskState::NoSlot);
        return;
    }
    if (m_n <= 0) {
        m_tick.stop();
        setState(TourTaskState::NoSource);
        return;
    }
    if (!inRunWindow()) {
        // 不在运行时段内直接过期，避免先播放一段时间再被判定过期。
        m_tick.stop();
        setState(TourTaskState::Expired);
        emit releaseRequested(QStringLiteral("*"));
        TourAuditLog::log(QStringLiteral("task_expired"));
        return;
    }

    setState(TourTaskState::Running);
    beginBatch();
}

TourScheduler::Batch TourScheduler::makeBatch(int index, const FillResult& plan)
{
    Batch batch;
    batch.index = index;
    batch.generation = ++m_generation;
    batch.keys = plan.keys;
    batch.batchDeadline = nowMs() + m_scheme.batchTimeoutMs;

    for (int i = 0; i < m_s; ++i) {
        SlotAssignment assignment;
        assignment.layoutItemId = m_unlockedItems.at(i);
        if (i < plan.items.size()) {
            assignment.channel = plan.items.at(i);
        }
        batch.assignments.append(assignment);
    }
    return batch;
}

QVector<TourChannelRef> TourScheduler::activateBatch()
{
    QVector<TourChannelRef> channels;
    for (const SlotAssignment& assignment : m_batch.assignments) {
        if (assignment.channel.isEmpty()) {
            emit windowStateChanged(assignment.layoutItemId, TourChannelState::Skipped,
                                    TourChannelRef(), QStringLiteral("本批无更多设备"));
            // 空槽位要释放上一批残留的画面，避免与“无更多设备”状态矛盾。
            emit releaseRequested(assignment.layoutItemId);
            continue;
        }
        channels.append(assignment.channel);
        emit windowStateChanged(assignment.layoutItemId, TourChannelState::Requesting,
                                assignment.channel, QString());
        emit playRequested(assignment.layoutItemId, assignment.channel, m_batch.generation);
    }

    emit batchStarted(m_batch.index, m_cycle, channels);
    m_tick.start();
    return channels;
}

void TourScheduler::beginBatch()
{
    m_gapDeadline = 0;
    const FillResult plan = TourBatchPlanner::fill(m_snapshot.activeQueue, m_cursor, m_s);
    m_cursor = plan.cursor;
    m_batch = makeBatch(m_batchHistory.size(), plan);
    const QVector<TourChannelRef> channels = activateBatch();

    QJsonObject detail;
    detail.insert(QStringLiteral("batch"), m_batch.index);
    detail.insert(QStringLiteral("cycle"), m_cycle);
    detail.insert(QStringLiteral("channels"), channels.size());
    TourAuditLog::log(QStringLiteral("batch_start"), detail);
}

void TourScheduler::replayBatch(const Batch& batch)
{
    m_gapDeadline = 0;
    m_batch = batch;
    m_batch.generation = ++m_generation;
    m_batch.displayable = false;
    m_batch.finished = false;
    m_batch.backfillUsed = 0;
    m_batch.replay = true;
    m_batch.batchDeadline = nowMs() + m_scheme.batchTimeoutMs;
    m_batch.dwellDeadline = 0;
    activateBatch();

    QJsonObject detail;
    detail.insert(QStringLiteral("batch"), m_batch.index);
    detail.insert(QStringLiteral("cycle"), m_cycle);
    detail.insert(QStringLiteral("replay"), true);
    TourAuditLog::log(QStringLiteral("batch_replay"), detail);
}

void TourScheduler::finishBatch()
{
    m_batch.finished = true;
    for (const SlotAssignment& assignment : m_batch.assignments) {
        if (!assignment.channel.isEmpty()) {
            m_covered.insert(assignment.channel.key());
        }
    }

    emit batchFinished(m_batch.index, m_cycle);
    // 回放的历史批次结束后不重复入历史，避免记录无限增长与翻页错位。
    if (!m_batch.replay) {
        m_batchHistory.append(m_batch);
    }

    if (windowExpired()) {
        setState(TourTaskState::Expired);
        emit releaseRequested(QStringLiteral("*"));
        TourAuditLog::log(QStringLiteral("task_expired"));
        return;
    }

    qint64 gap = m_scheme.switchGapMs;
    if (m_covered.size() >= m_n) {
        m_cycle += 1;
        m_covered.clear();
        emit cycleAdvanced(m_cycle);
        if (m_scheme.maxCycles > 0 && m_cycle >= m_scheme.maxCycles) {
            setState(TourTaskState::Completed);
            emit releaseRequested(QStringLiteral("*"));
            TourAuditLog::log(QStringLiteral("task_completed"));
            return;
        }
        if (m_scheme.cycleGapMs > 0) {
            gap = m_scheme.cycleGapMs;
        }
    }

    if (gap <= 0) {
        beginBatch();
    } else {
        m_gapDeadline = nowMs() + gap;
        // 换批间隔内不再有 dwell/首帧计时，倒计时应归零。
        emit countdown(0, 0);
    }
}

void TourScheduler::onTick()
{
    if (m_state != TourTaskState::Running) {
        return;
    }
    const qint64 now = nowMs();

    if (m_gapDeadline > 0) {
        if (now >= m_gapDeadline) {
            m_gapDeadline = 0;
            beginBatch();
        }
        return;
    }

    if (!m_batch.displayable && now >= m_batch.batchDeadline) {
        // 首帧超时：批次按可展示继续，仍在等待的窗口标记为失败并尝试补位，
        // 避免这批窗口一直黑屏。
        m_batch.displayable = true;
        m_batch.dwellDeadline = now + m_scheme.dwellMs;
        QStringList timedOutSlots;
        for (const SlotAssignment& assignment : m_batch.assignments) {
            if (assignment.channel.isEmpty()) {
                continue;
            }
            if (m_failed.contains(assignment.channel.key())) {
                continue;
            }
            m_failed.insert(assignment.channel.key());
            // 失败也算“已尝试”，否则永久失败的通道会让周期永远无法推进。
            m_covered.insert(assignment.channel.key());
            emit windowStateChanged(assignment.layoutItemId, TourChannelState::Failed,
                                    assignment.channel, QStringLiteral("首帧超时"));
            timedOutSlots.append(assignment.layoutItemId);
        }
        for (const QString& layoutItemId : timedOutSlots) {
            backfillSlot(layoutItemId);
        }
    }

    if (m_batch.displayable && now >= m_batch.dwellDeadline) {
        finishBatch();
        return;
    }

    const int dwellRemain = m_batch.displayable ? static_cast<int>(m_batch.dwellDeadline - now) : 0;
    const int timeoutRemain = m_batch.displayable ? 0 : static_cast<int>(m_batch.batchDeadline - now);
    emit countdown(dwellRemain, timeoutRemain);
}

int TourScheduler::slotIndexOf(const QString& layoutItemId) const
{
    for (int i = 0; i < m_batch.assignments.size(); ++i) {
        if (m_batch.assignments.at(i).layoutItemId == layoutItemId) {
            return i;
        }
    }
    return -1;
}

QString TourScheduler::layoutItemIdForChannel(const TourChannelRef& channel) const
{
    for (const SlotAssignment& assignment : m_batch.assignments) {
        if (assignment.channel == channel) {
            return assignment.layoutItemId;
        }
    }
    return QString();
}

void TourScheduler::onFirstFrame(const QString& layoutItemId, quint64 generation)
{
    if (generation != m_batch.generation || m_batch.finished) {
        return;
    }
    const int index = slotIndexOf(layoutItemId);
    if (index < 0) {
        return;
    }
    const TourChannelRef channel = m_batch.assignments.at(index).channel;
    if (channel.isEmpty()) {
        return;
    }
    // 首帧晚于超时到达时，之前的失败标记应撤销。
    m_failed.remove(channel.key());
    emit windowStateChanged(layoutItemId, TourChannelState::Playing, channel, QString());
    if (!m_batch.displayable) {
        m_batch.displayable = true;
        m_batch.dwellDeadline = nowMs() + m_scheme.dwellMs;
    }
}

void TourScheduler::onPlayFailed(const QString& layoutItemId, quint64 generation, const QString& reason)
{
    if (generation != m_batch.generation || m_batch.finished) {
        return;
    }
    const int index = slotIndexOf(layoutItemId);
    if (index < 0) {
        return;
    }
    const TourChannelRef channel = m_batch.assignments.at(index).channel;
    if (channel.isEmpty()) {
        return;
    }
    m_failed.insert(channel.key());
    // 失败也算“已尝试”，否则永久失败的通道会让周期永远无法推进。
    m_covered.insert(channel.key());
    emit windowStateChanged(layoutItemId, TourChannelState::Failed, channel, reason);

    QJsonObject detail;
    detail.insert(QStringLiteral("channel"), channel.key());
    detail.insert(QStringLiteral("reason"), reason);
    TourAuditLog::log(QStringLiteral("channel_failed"), detail);

    if (m_state == TourTaskState::Running) {
        backfillSlot(layoutItemId);
    }
}

void TourScheduler::onDisconnected(const QString& layoutItemId, quint64 generation)
{
    onPlayFailed(layoutItemId, generation, QStringLiteral("连接断开"));
}

void TourScheduler::onStalled(const QString& layoutItemId, quint64 generation)
{
    onPlayFailed(layoutItemId, generation, QStringLiteral("画面卡顿"));
}

void TourScheduler::backfillSlot(const QString& layoutItemId)
{
    if (m_state != TourTaskState::Running || m_batch.finished) {
        return;
    }
    if (m_batch.backfillUsed >= m_scheme.backfillLimit) {
        emit windowStateChanged(layoutItemId, TourChannelState::Failed,
                                TourChannelRef(), QStringLiteral("已达补位上限"));
        return;
    }

    int newCursor = m_cursor;
    const TourChannelRef replacement = TourBatchPlanner::backfill(
        m_snapshot.activeQueue, m_cursor, m_batch.keys, m_failed,
        m_scheme.backfillLimit, m_batch.backfillUsed, &newCursor, &m_covered);

    if (replacement.isEmpty()) {
        emit windowStateChanged(layoutItemId, TourChannelState::Skipped, TourChannelRef(),
                                QStringLiteral("本批无更多设备"));
        return;
    }

    m_cursor = newCursor;
    m_batch.backfillUsed += 1;
    m_batch.keys.insert(replacement.key());
    m_covered.insert(replacement.key());

    const int index = slotIndexOf(layoutItemId);
    if (index >= 0) {
        m_batch.assignments[index].channel = replacement;
    }
    emit windowStateChanged(layoutItemId, TourChannelState::Backfilling, replacement, QString());
    emit playRequested(layoutItemId, replacement, m_batch.generation);

    QJsonObject detail;
    detail.insert(QStringLiteral("from"), layoutItemId);
    detail.insert(QStringLiteral("channel"), replacement.key());
    TourAuditLog::log(QStringLiteral("channel_backfill"), detail);
}

void TourScheduler::pause()
{
    if (m_state != TourTaskState::Running) {
        return;
    }
    const qint64 now = nowMs();
    m_frozenDwell = m_batch.displayable ? std::max<qint64>(0, m_batch.dwellDeadline - now) : -1;
    m_frozenTimeout = m_batch.displayable ? -1 : std::max<qint64>(0, m_batch.batchDeadline - now);
    m_tick.stop();
    setState(TourTaskState::Paused);
    // 暂停时计时冻结，倒计时归零避免界面继续显示旧值。
    emit countdown(0, 0);
    TourAuditLog::log(QStringLiteral("task_pause"));
}

void TourScheduler::resume()
{
    if (m_state != TourTaskState::Paused) {
        return;
    }
    const qint64 now = nowMs();
    if (m_batch.displayable && m_frozenDwell >= 0) {
        m_batch.dwellDeadline = now + m_frozenDwell;
    } else if (m_frozenTimeout >= 0) {
        m_batch.batchDeadline = now + m_frozenTimeout;
    }
    m_frozenDwell = -1;
    m_frozenTimeout = -1;
    m_tick.start();
    setState(TourTaskState::Running);

    // 恢复后重新为失败窗口补位。
    for (const SlotAssignment& assignment : m_batch.assignments) {
        if (!assignment.channel.isEmpty() && m_failed.contains(assignment.channel.key())) {
            backfillSlot(assignment.layoutItemId);
        }
    }
    TourAuditLog::log(QStringLiteral("task_resume"));
}

void TourScheduler::stop()
{
    m_tick.stop();
    m_gapDeadline = 0;
    setState(TourTaskState::Stopped);
    emit releaseRequested(QStringLiteral("*"));
    TourAuditLog::log(QStringLiteral("task_stop"));
}

void TourScheduler::nextBatch()
{
    // 暂停时计时被冻结，翻页后无法推进，直接忽略。
    if (m_state != TourTaskState::Running) {
        return;
    }

    if (m_batch.index + 1 < m_batchHistory.size()) {
        replayBatch(m_batchHistory.at(m_batch.index + 1));
        return;
    }

    const FillResult plan = TourBatchPlanner::fill(m_snapshot.activeQueue, m_cursor, m_s);
    if (plan.items.isEmpty()) {
        emit pageAtEnd();
        return;
    }

    m_cursor = plan.cursor;
    m_gapDeadline = 0;
    m_batch = makeBatch(m_batch.index + 1, plan);
    const QVector<TourChannelRef> channels = activateBatch();

    // 队列回绕到起点时提示已到队尾。
    if (!channels.isEmpty() && !m_batchHistory.isEmpty()
        && !m_batchHistory.first().assignments.isEmpty()
        && channels.first().key() == m_batchHistory.first().assignments.first().channel.key()) {
        emit pageAtEnd();
    }
    TourAuditLog::log(QStringLiteral("batch_next"));
}

void TourScheduler::prevBatch()
{
    // 暂停时计时被冻结，翻页后无法推进，直接忽略。
    if (m_state != TourTaskState::Running) {
        return;
    }
    if (m_batch.index <= 0 || m_batchHistory.isEmpty()) {
        emit pageAtEnd();
        return;
    }
    // 手动“下一批”不会入历史，当前序号可能超过历史长度；夹到最后一条有效记录。
    int target = m_batch.index - 1;
    if (target >= m_batchHistory.size()) {
        target = m_batchHistory.size() - 1;
    }
    if (target < 0) {
        emit pageAtEnd();
        return;
    }
    replayBatch(m_batchHistory.at(target));
    TourAuditLog::log(QStringLiteral("batch_prev"));
}

void TourScheduler::retrySlot(const QString& layoutItemId)
{
    if (m_batch.finished) {
        return;
    }
    const int index = slotIndexOf(layoutItemId);
    if (index < 0) {
        return;
    }
    const TourChannelRef channel = m_batch.assignments.at(index).channel;
    if (channel.isEmpty()) {
        return;
    }
    m_failed.remove(channel.key());
    emit windowStateChanged(layoutItemId, TourChannelState::Requesting, channel, QString());
    emit playRequested(layoutItemId, channel, m_batch.generation);
    TourAuditLog::log(QStringLiteral("channel_retry"));
}

void TourScheduler::setFullscreenPaused(bool paused)
{
    if (paused) {
        if (m_state == TourTaskState::Running) {
            pause();
            m_fullscreenPaused = true;
        }
        return;
    }
    if (m_fullscreenPaused) {
        m_fullscreenPaused = false;
        if (m_state == TourTaskState::Paused) {
            resume();
        }
    }
}

bool TourScheduler::inRunWindowAt(const RunWindow& window, const QDateTime& now)
{
    const QTime start = parseTime(window.start);
    const QTime end = parseTime(window.end);
    if (!start.isValid() || !end.isValid()) {
        // 时段非法时不做限制，避免误停。
        return true;
    }

    const QDate date = now.date();
    const QTime time = now.time();
    const auto dayAllowed = [&window](const QDate& day) {
        return window.weekdays.isEmpty() || window.weekdays.contains(day.dayOfWeek());
    };

    if (window.crossDay || end < start) {
        if (time >= start) {
            return dayAllowed(date);
        }
        if (time <= end) {
            // 凌晨时段属于前一天开启的运行窗口，星期也应按前一天判断。
            return dayAllowed(date.addDays(-1));
        }
        return false;
    }
    return dayAllowed(date) && time >= start && time <= end;
}

bool TourScheduler::inRunWindow() const
{
    if (!m_scheme.hasRunWindow) {
        return true;
    }
    return inRunWindowAt(m_scheme.runWindow, QDateTime::currentDateTime());
}

bool TourScheduler::windowExpired() const
{
    if (!inRunWindow()) {
        return true;
    }
    if (m_scheme.maxCycles == 0 && m_scheme.safetyMaxDurationMs > 0) {
        if (nowMs() > m_scheme.safetyMaxDurationMs) {
            return true;
        }
    }
    return false;
}

} // namespace rustcam
