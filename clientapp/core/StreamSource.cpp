#include "StreamSource.h"
#include "ReplayPlayer.h"
#include "common/Logger.h"

namespace rustcam {

StreamSource::StreamSource(const QString& deviceId, QObject* parent)
    : QObject(parent)
    , m_deviceId(deviceId)
    , m_currentClipIndex(-1)
    , m_state(State::Stopped)
{
}

StreamSource::~StreamSource() {
    if (m_player) {
        disconnect(m_player, nullptr, this, nullptr);
    }
}

void StreamSource::setClips(const QList<Clip>& clips) {
    QMutexLocker locker(&m_mutex);
    m_clips = clips;
    m_currentClipIndex = -1;
    LOG_INFO("StreamSource", QString("setClips: deviceId=%1, clipCount=%2").arg(m_deviceId).arg(clips.size()));
}

void StreamSource::setPlayer(ReplayPlayer* player) {
    QMutexLocker locker(&m_mutex);
    if (m_player) {
        disconnect(m_player, nullptr, this, nullptr);
    }
    m_player = player;

    if (m_player) {
        connect(m_player, &ReplayPlayer::stateChanged, this, [this](ReplayPlayer::State newState) {
            onPlayerStateChanged(ReplayPlayer::State::Idle, newState);
        });
        connect(m_player, &ReplayPlayer::progressChanged, this, [this](int64_t currentMs, int64_t durationMs) {
            onPlayerProgress(currentMs, durationMs);
        });
        connect(m_player, &ReplayPlayer::completed, this, [this]() {
            onPlayerCompleted();
        });
    }
}

void StreamSource::play() {
    QMutexLocker locker(&m_mutex);
    if (m_clips.isEmpty()) {
        setState(State::NoRecording);
        return;
    }

    if (m_currentClipIndex < 0) {
        m_currentClipIndex = 0;
        const Clip& clip = m_clips[m_currentClipIndex];
        emit clipChanged(-1, m_currentClipIndex);
        if (m_player) {
            m_player->loadFile(clip.url);
        }
    }

    if (m_player) {
        m_player->play();
    }
    setState(State::Playing);
}

void StreamSource::pause() {
    QMutexLocker locker(&m_mutex);
    if (m_player) {
        m_player->pause();
    }
    setState(State::Paused);
}

void StreamSource::stop() {
    QMutexLocker locker(&m_mutex);
    if (m_player) {
        m_player->stop();
    }
    m_currentClipIndex = -1;
    m_baseTimeMs = 0;
    setState(State::Stopped);
}

void StreamSource::seekTo(int64_t absoluteTimeMs) {
    QMutexLocker locker(&m_mutex);

    int clipIdx = findClipAt(absoluteTimeMs);
    if (clipIdx < 0) {
        int nearestIdx = findNearestClip(absoluteTimeMs);
        if (nearestIdx >= 0) {
            const Clip& nearest = m_clips[nearestIdx];
            int64_t gapMs = qAbs(absoluteTimeMs -
                (absoluteTimeMs < nearest.startTimeMs ? nearest.startTimeMs : nearest.endTimeMs));
            if (gapMs > 5000) {
                LOG_WARNING("StreamSource", QString("Gap detected: %1ms at time %2").arg(gapMs).arg(absoluteTimeMs));
                emit gapDetected(absoluteTimeMs, gapMs);
            }
        }
        stop();
        setState(State::NoRecording);
        if (m_noRecordingCallback) {
            m_noRecordingCallback(absoluteTimeMs);
        }
        emit noRecording(absoluteTimeMs);
        return;
    }

    if (clipIdx != m_currentClipIndex) {
        switchToClip(clipIdx);
    }

    if (m_player && m_currentClipIndex >= 0 && m_currentClipIndex < m_clips.size()) {
        const Clip& clip = m_clips[m_currentClipIndex];
        int64_t clipOffsetMs = absoluteTimeMs - clip.startTimeMs;
        m_player->seek(clipOffsetMs);
    }

    m_baseTimeMs = absoluteTimeMs;
}

void StreamSource::setSpeed(double speed) {
    QMutexLocker locker(&m_mutex);
    if (m_player) {
        m_player->setSpeed(speed);
    }
}

bool StreamSource::isInRange(int64_t absoluteTimeMs) const {
    return findClipAt(absoluteTimeMs) >= 0;
}

int64_t StreamSource::totalDurationMs() const {
    if (m_clips.isEmpty()) return 0;
    return m_clips.last().endTimeMs - m_clips.first().startTimeMs;
}

int StreamSource::findClipAt(int64_t absoluteTimeMs) const {
    for (int i = 0; i < m_clips.size(); ++i) {
        const Clip& clip = m_clips[i];
        if (absoluteTimeMs >= clip.startTimeMs && absoluteTimeMs < clip.endTimeMs) {
            return i;
        }
    }
    return -1;
}

int StreamSource::findNearestClip(int64_t absoluteTimeMs) const {
    if (m_clips.isEmpty()) return -1;
    int nearestIdx = -1;
    int64_t minDist = INT64_MAX;
    for (int i = 0; i < m_clips.size(); ++i) {
        const Clip& clip = m_clips[i];
        int64_t dist = qAbs(absoluteTimeMs - clip.startTimeMs);
        if (dist < minDist) {
            minDist = dist;
            nearestIdx = i;
        }
    }
    return nearestIdx;
}

void StreamSource::switchToClip(int clipIndex) {
    if (clipIndex < 0 || clipIndex >= m_clips.size()) return;

    int oldIndex = m_currentClipIndex;
    m_currentClipIndex = clipIndex;

    const Clip& clip = m_clips[clipIndex];
    LOG_INFO("StreamSource", QString("switchToClip: deviceId=%1, clipIndex=%2, url=%3, start=%4, end=%5")
        .arg(m_deviceId).arg(clipIndex).arg(clip.url).arg(clip.startTimeMs).arg(clip.endTimeMs));

    emit clipChanged(oldIndex, clipIndex);

    if (m_player) {
        m_player->stop();
        m_player->loadFile(clip.url);
    }
}

void StreamSource::onPlayerStateChanged(ReplayPlayer::State oldState, ReplayPlayer::State newState) {
    QMutexLocker locker(&m_mutex);

    switch (newState) {
        case ReplayPlayer::State::Playing:
            setState(State::Playing);
            break;
        case ReplayPlayer::State::Paused:
            setState(State::Paused);
            break;
        case ReplayPlayer::State::Completed:
            if (m_currentClipIndex >= 0 && m_currentClipIndex < m_clips.size() - 1) {
                int oldIndex = m_currentClipIndex;
                switchToClip(m_currentClipIndex + 1);
                if (m_player) {
                    m_player->play();
                }
            } else {
                setState(State::Stopped);
                if (m_clipFinishedCallback) {
                    m_clipFinishedCallback();
                }
            }
            break;
        case ReplayPlayer::State::Error:
        case ReplayPlayer::State::Idle:
            setState(State::Stopped);
            break;
        default:
            break;
    }
}

void StreamSource::onPlayerProgress(int64_t currentMs, int64_t durationMs) {
    QMutexLocker locker(&m_mutex);
    if (m_currentClipIndex >= 0 && m_currentClipIndex < m_clips.size()) {
        const Clip& clip = m_clips[m_currentClipIndex];
        m_baseTimeMs = clip.startTimeMs + currentMs;
        m_lastProgressMs = currentMs;
        emit playbackProgressChanged(m_baseTimeMs);
    }
}

void StreamSource::onPlayerCompleted() {
    QMutexLocker locker(&m_mutex);
    if (m_currentClipIndex >= 0 && m_currentClipIndex < m_clips.size() - 1) {
        int oldIndex = m_currentClipIndex;
        switchToClip(m_currentClipIndex + 1);
        if (m_player) {
            m_player->play();
        }
    } else {
        setState(State::Stopped);
        if (m_clipFinishedCallback) {
            m_clipFinishedCallback();
        }
    }
}

void StreamSource::setStateChangeCallback(StateChangeCallback cb) {
    m_stateChangeCallback = std::move(cb);
}

void StreamSource::setNoRecordingCallback(NoRecordingCallback cb) {
    m_noRecordingCallback = std::move(cb);
}

void StreamSource::setClipFinishedCallback(ClipFinishedCallback cb) {
    m_clipFinishedCallback = std::move(cb);
}

void StreamSource::setState(State state) {
    if (m_state != state) {
        State oldState = m_state;
        m_state = state;
        emit stateChanged(state);
        if (m_stateChangeCallback) {
            m_stateChangeCallback(oldState, state);
        }
    }
}

}
