#include "PlaybackController.h"
#include "common/Logger.h"
#include <QTimer>

namespace rustcam {

PlaybackGroup::PlaybackGroup(int groupId, QObject* parent)
    : QObject(parent)
    , m_groupId(groupId)
{
}

PlaybackGroup::~PlaybackGroup() {
    clearCells();
}

void PlaybackGroup::addCell(StreamSource* cell) {
    if (cell && !m_cells.contains(cell)) {
        m_cells.append(cell);
        emit cellAdded(cell);
    }
}

void PlaybackGroup::removeCell(StreamSource* cell) {
    if (cell) {
        m_cells.removeAll(cell);
        emit cellRemoved(cell);
    }
}

void PlaybackGroup::clearCells() {
    m_cells.clear();
}

void PlaybackGroup::syncTo(int64_t baseTimeMs) {
    m_baseTimeMs = baseTimeMs;
    for (StreamSource* cell : m_cells) {
        if (cell->state() != StreamSource::State::Playing) {
            continue;
        }
        if (qAbs(cell->baseTimeMs() - baseTimeMs) < 100) {
            continue;
        }
        if (cell->isInRange(baseTimeMs)) {
            cell->seekTo(baseTimeMs);
            cell->play();
        } else {
            cell->stop();
        }
    }
}

void PlaybackGroup::seekTo(int64_t baseTimeMs) {
    m_baseTimeMs = baseTimeMs;
    for (StreamSource* cell : m_cells) {
        if (cell->clips().isEmpty()) {
            continue;
        }
        // 越界时 StreamSource::seekTo 会停播并给出「无录像」提示
        cell->seekTo(baseTimeMs);
    }
}

void PlaybackGroup::play() {
    for (StreamSource* cell : m_cells) {
        if (cell->clips().isEmpty()) {
            continue;
        }

        if (cell->state() == StreamSource::State::Paused) {
            cell->play();
            continue;
        }

        if (cell->state() == StreamSource::State::Stopped ||
            cell->state() == StreamSource::State::NoRecording) {
            if (cell->isInRange(m_baseTimeMs)) {
                cell->seekTo(m_baseTimeMs);
            }
            cell->play();
        }
    }
}

void PlaybackGroup::pause() {
    for (StreamSource* cell : m_cells) {
        if (cell->state() == StreamSource::State::Playing) {
            cell->pause();
        }
    }
}

void PlaybackGroup::stop() {
    for (StreamSource* cell : m_cells) {
        cell->stop();
    }
}

PlaybackController::PlaybackController(QObject* parent)
    : QObject(parent)
    , m_defaultGroup(nullptr)
{
    m_defaultGroup = new PlaybackGroup(0, this);
    m_groups.append(m_defaultGroup);
}

PlaybackController::~PlaybackController() {
    for (PlaybackGroup* group : m_groups) {
        delete group;
    }
    m_groups.clear();
}

void PlaybackController::setBaseTime(int64_t timeMs) {
    if (m_baseTimeMs == timeMs) {
        for (PlaybackGroup* group : m_groups) {
            group->setBaseTime(timeMs);
        }
        return;
    }

    if (m_syncState == SyncState::Syncing) {
        return;
    }

    m_baseTimeMs = timeMs;
    emit baseTimeChanged(timeMs);

    m_syncState = SyncState::Syncing;
    emit syncStateChanged(m_syncState);

    m_batchStartIndex = 0;
    m_batchTimeMs = timeMs;
    onBatchSync();
}

void PlaybackController::onBatchSync() {
    int endIndex = qMin(m_batchStartIndex + BATCH_SIZE, m_groups.size());

    for (int i = m_batchStartIndex; i < endIndex; ++i) {
        m_groups[i]->syncTo(m_batchTimeMs);
    }

    m_batchStartIndex = endIndex;

    if (m_batchStartIndex < m_groups.size()) {
        QTimer::singleShot(0, this, &PlaybackController::onBatchSync);
    } else {
        m_syncState = SyncState::Synced;
        emit syncStateChanged(m_syncState);
    }
}

void PlaybackController::setGroupBaseTime(PlaybackGroup* group, int64_t timeMs) {
    if (!group) {
        return;
    }
    // 通道之间播放位置互相独立：只同步目标组
    m_baseTimeMs = timeMs;
    group->seekTo(timeMs);
    emit baseTimeChanged(timeMs);
}

int64_t PlaybackController::groupBaseTime(PlaybackGroup* group) const {
    return group ? group->baseTimeMs() : 0;
}

void PlaybackController::addGroup(PlaybackGroup* group) {
    if (group && !m_groups.contains(group)) {
        m_groups.append(group);
        emit groupAdded(group);
    }
}

void PlaybackController::removeGroup(PlaybackGroup* group) {
    if (group && group != m_defaultGroup) {
        m_groups.removeAll(group);
        emit groupRemoved(group->groupId());
        delete group;
    }
}

void PlaybackController::removeGroup(int groupId) {
    if (groupId == 0) return;
    for (int i = 0; i < m_groups.size(); ++i) {
        if (m_groups[i]->groupId() == groupId) {
            PlaybackGroup* group = m_groups.takeAt(i);
            emit groupRemoved(groupId);
            delete group;
            return;
        }
    }
}

void PlaybackController::play() {
    for (PlaybackGroup* group : m_groups) {
        group->play();
    }
}

void PlaybackController::pause() {
    for (PlaybackGroup* group : m_groups) {
        group->pause();
    }
}

void PlaybackController::stop() {
    for (PlaybackGroup* group : m_groups) {
        group->stop();
    }
}

void PlaybackController::setSpeed(double speed) {
    for (PlaybackGroup* group : m_groups) {
        for (StreamSource* cell : group->cells()) {
            cell->setSpeed(speed);
        }
    }
}

void PlaybackController::setGroupSpeed(PlaybackGroup* group, double speed) {
    if (!group) {
        return;
    }
    for (StreamSource* cell : group->cells()) {
        cell->setSpeed(speed);
    }
}

PlaybackGroup* PlaybackController::createGroup() {
    QMutexLocker locker(&m_mutex);
    PlaybackGroup* group = new PlaybackGroup(m_nextGroupId++, this);
    m_groups.append(group);
    emit groupAdded(group);
    return group;
}

void PlaybackController::destroyGroup(int groupId) {
    if (groupId == 0) return;
    removeGroup(groupId);
}

}
