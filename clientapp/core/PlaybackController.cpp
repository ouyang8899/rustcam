#include "PlaybackController.h"
#include "common/Logger.h"

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
    for (StreamSource* cell : m_cells) {
        if (cell->isInRange(baseTimeMs)) {
            cell->seekTo(baseTimeMs);
            cell->play();
        } else {
            cell->stop();
        }
    }
}

void PlaybackGroup::play() {
    for (StreamSource* cell : m_cells) {
        if (cell->state() == StreamSource::State::Paused) {
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
    if (m_baseTimeMs != timeMs) {
        m_baseTimeMs = timeMs;
        emit baseTimeChanged(timeMs);

        for (PlaybackGroup* group : m_groups) {
            group->syncTo(timeMs);
        }
    }
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
