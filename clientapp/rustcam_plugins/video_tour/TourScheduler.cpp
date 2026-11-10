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
    m_lockedItems = lockedItemIds;

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

    setState(TourTaskState::Running);
    beginBatch();
}

void TourScheduler::beginBatch()
{
    const FillResult plan = TourBatchPlanner::fill(m_snapshot.activeQueue, m_cursor, m_s);
    m_cursor = plan.cursor;

    Batch batch;
    batch.index = m_batchHistory.size();
    batch.generation = ++m_generation;
    batch.keys = plan.keys;
    batch.batchDeadline = nowMs() + m_scheme.batchTimeoutMs;

    QVector<TourChannelRef> channels;
    for (int i = 0; i < m_s; ++i) {
        SlotAssignment assignment;
        assignment.layoutItemId = m_unlockedItems.at(i);
        if (i < plan.items.size()) {
            assignment.channel = plan.items.at(i);
        }
        batch.assignments.append(assignment);
        if (!assignment.channel.isEmpty()) {
            channels.append(assignment.channel);
        }
    }

    m_batch = batch;

    for (const SlotAssignment& assignment : m_batch.assignments) {
        if (assignment.channel.isEmpty()) {
            emit windowStateChanged(assignment.layoutItemId, TourChannelState::Skipped,
                                    TourChannelRef(), QStringLiteral("本批无更多设备"));
            continue;
        }
        emit windowStateChanged(assignment.layoutItemId, TourChannelState::Requesting,
                                assignment.channel, QString());
        emit playRequested(assignment.layoutItemId, assignment.channel, m_batch.generation);
    }

    emit batchStarted(m_batch.index, m_cycle, channels);
    m_tick.start();

    QJsonObject detail;
    detail.insert(QStringLiteral("batch"), m_batch.index);
    detail.insert(QStringLiteral("cycle"), m_cycle);
    detail.insert(QStringLiteral("channels"), channels.size());
    TourAuditLog::log(QStringLiteral("batch_start"), detail);
}

void TourScheduler::replayBatch(const Batch& batch)
{
    m_batch = batch;
    m_batch.generation = ++m_generation;
    m_batch.displayable = false;
    m_batch.backfillUsed = 0;
    m_batch.batchDeadline = nowMs() + m_scheme.batchTimeoutMs;
    m_batch.dwellDeadline = 0;

    QVector<TourChannelRef> channels;
    for (const SlotAssignment& assignment : m_batch.assignments) {
        if (assignment.channel.isEmpty()) {
            emit windowStateChanged(assignment.layoutItemId, TourChannelState::Skipped,
                                    TourChannelRef(), QStringLiteral("本批无更多设备"));
            continue;
        }
        channels.append(assignment.channel);
        emit windowStateChanged(assignment.layoutItemId, TourChannelState::Requesting,
                                assignment.channel, QString());
        emit playRequested(assignment.layoutItemId, assignment.channel, m_batch.generation);
    }

    emit batchStarted(m_batch.index, m_cycle, channels);
    m_tick.start();

    QJsonObject detail;
    detail.insert(QStringLiteral("batch"), m_batch.index);
    detail.insert(QStringLiteral("cycle"), m_cycle);
    detail.insert(QStringLiteral("replay"), true);
    TourAuditLog::log(QStringLiteral("batch_replay"), detail);
}

void TourScheduler::finishBatch()
{
    for (const SlotAssignment& assignment : m_batch.assignments) {
        if (!assignment.channel.isEmpty()) {
            m_covered.insert(assignment.channel.key());
        }
    }

    emit batchFinished(m_batch.index, m_cycle);
    m_batchHistory.append(m_batch);

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
        // 首帧超时：批次按可展示继续，仍在等待的窗口标记为失败。
        m_batch.displayable = true;
        m_batch.dwellDeadline = now + m_scheme.dwellMs;
        for (const SlotAssignment& assignment : m_batch.assignments) {
            if (assignment.channel.isEmpty()) {
                continue;
            }
            if (m_failed.contains(assignment.channel.key())) {
                continue;
            }
            m_failed.insert(assignment.channel.key());
            emit windowStateChanged(assignment.layoutItemId, TourChannelState::Failed,
                                    assignment.channel, QStringLiteral("首帧超时"));
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
    if (generation != m_batch.generation) {
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
    emit windowStateChanged(layoutItemId, TourChannelState::Playing, channel, QString());
    if (!m_batch.displayable) {
        m_batch.displayable = true;
        m_batch.dwellDeadline = nowMs() + m_scheme.dwellMs;
    }
}

void TourScheduler::onPlayFailed(const QString& layoutItemId, quint64 generation, const QString& reason)
{
    if (generation != m_batch.generation) {
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
    if (m_state != TourTaskState::Running) {
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
        m_scheme.backfillLimit, m_batch.backfillUsed, &newCursor);

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
    if (m_state != TourTaskState::Running && m_state != TourTaskState::Paused) {
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
    Batch batch;
    batch.index = m_batch.index + 1;
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
    m_batch = batch;

    QVector<TourChannelRef> channels;
    for (const SlotAssignment& assignment : m_batch.assignments) {
        if (assignment.channel.isEmpty()) {
            emit windowStateChanged(assignment.layoutItemId, TourChannelState::Skipped,
                                    TourChannelRef(), QStringLiteral("本批无更多设备"));
            continue;
        }
        channels.append(assignment.channel);
        emit windowStateChanged(assignment.layoutItemId, TourChannelState::Requesting,
                                assignment.channel, QString());
        emit playRequested(assignment.layoutItemId, assignment.channel, m_batch.generation);
    }
    emit batchStarted(m_batch.index, m_cycle, channels);
    m_tick.start();

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
    if (m_state != TourTaskState::Running && m_state != TourTaskState::Paused) {
        return;
    }
    if (m_batch.index <= 0 || m_batchHistory.isEmpty()) {
        emit pageAtEnd();
        return;
    }
    replayBatch(m_batchHistory.at(m_batch.index - 1));
    TourAuditLog::log(QStringLiteral("batch_prev"));
}

void TourScheduler::retrySlot(const QString& layoutItemId)
{
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

bool TourScheduler::inRunWindow() const
{
    if (!m_scheme.hasRunWindow) {
        return true;
    }
    const QTime start = parseTime(m_scheme.runWindow.start);
    const QTime end = parseTime(m_scheme.runWindow.end);
    if (!start.isValid() || !end.isValid()) {
        // 时段非法时不做限制，避免误停。
        return true;
    }
    if (!m_scheme.runWindow.weekdays.isEmpty()) {
        const int today = QDateTime::currentDateTime().date().dayOfWeek();
        if (!m_scheme.runWindow.weekdays.contains(today)) {
            return false;
        }
    }
    const QTime now = QDateTime::currentDateTime().time();
    if (m_scheme.runWindow.crossDay || end < start) {
        return now >= start || now <= end;
    }
    return now >= start && now <= end;
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
