#include "StreamSource.h"
#include "ReplayPlayer.h"
#include "common/Logger.h"
#include <algorithm>

namespace rustcam {

StreamSource::StreamSource(const QString& deviceId, ReplayPlayer* player, QObject* parent)
    : QObject(parent)
    , m_deviceId(deviceId)
    , m_player(player)
    , m_currentClipIndex(-1)
    , m_state(State::Stopped)
{
    // 播放器在构造时传入，连接一次建立，之后不再变动
    if (m_player) {
        connect(m_player, &ReplayPlayer::stateChanged, this, &StreamSource::onPlayerStateChanged);
        connect(m_player, &ReplayPlayer::progressChanged, this, &StreamSource::onPlayerProgress);
    }
}

StreamSource::~StreamSource() {
    // 连接随对象自动断开，不需要手动 disconnect
}

void StreamSource::setClips(const QList<Clip>& clips) {
    if (m_player && (m_state == State::Playing || m_state == State::Paused)) {
        m_player->stop();
    }
    m_clips = clips;
    sortClips();
    m_currentClipIndex = -1;
    if (m_state != State::Stopped) {
        setState(State::Stopped);
    }
    LOG_INFO("StreamSource", QString("setClips: deviceId=%1, clipCount=%2").arg(m_deviceId).arg(m_clips.size()));
}

/// 分片按开始时间升序：连续播放才按时间正序推进
void StreamSource::sortClips() {
    std::sort(m_clips.begin(), m_clips.end(), [](const Clip& a, const Clip& b) {
        return a.startTimeMs < b.startTimeMs;
    });
}

/// 追加分片（加载更多时用，不打断当前播放）
void StreamSource::appendClips(const QList<Clip>& clips) {
    if (clips.isEmpty()) {
        return;
    }
    // 记住当前片段，合并后按 url 找回它的位置
    QString currentUrl;
    if (m_currentClipIndex >= 0 && m_currentClipIndex < m_clips.size()) {
        currentUrl = m_clips[m_currentClipIndex].url;
    }
    for (const Clip& clip : clips) {
        bool exists = false;
        for (const Clip& exist : m_clips) {
            if (exist.url == clip.url) {
                exists = true;
                break;
            }
        }
        if (!exists) {
            m_clips.append(clip);
        }
    }
    sortClips();
    m_currentClipIndex = -1;
    if (!currentUrl.isEmpty()) {
        for (int i = 0; i < m_clips.size(); ++i) {
            if (m_clips[i].url == currentUrl) {
                m_currentClipIndex = i;
                break;
            }
        }
    }
    LOG_INFO("StreamSource", QString("appendClips: deviceId=%1, clipCount=%2").arg(m_deviceId).arg(m_clips.size()));
}

void StreamSource::play() {
    if (m_clips.isEmpty()) {
        setState(State::NoRecording);
        return;
    }

    if (m_currentClipIndex < 0) {
        m_currentClipIndex = 0;
        emit clipChanged(-1, m_currentClipIndex);
        if (m_player) {
            m_player->loadFile(m_clips[0].url, 0);
        }
    } else if (m_player) {
        m_player->play();
    }
    setState(State::Playing);
}

void StreamSource::pause() {
    if (m_player) {
        m_player->pause();
    }
    setState(State::Paused);
}

void StreamSource::stop() {
    if (m_player) {
        m_player->stop();
    }
    m_currentClipIndex = -1;
    m_baseTimeMs = 0;
    setState(State::Stopped);
}

void StreamSource::seekTo(int64_t absoluteTimeMs) {
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
        emit noRecording(absoluteTimeMs);
        return;
    }

    if (clipIdx != m_currentClipIndex) {
        const int64_t clipOffsetMs = absoluteTimeMs - m_clips[clipIdx].startTimeMs;
        switchToClip(clipIdx, clipOffsetMs);
    } else if (m_player) {
        const int64_t clipOffsetMs = absoluteTimeMs - m_clips[clipIdx].startTimeMs;
        m_player->seek(clipOffsetMs);
    }

    m_baseTimeMs = absoluteTimeMs;
}

void StreamSource::setSpeed(double speed) {
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

void StreamSource::switchToClip(int clipIndex, int64_t startOffsetMs) {
    if (clipIndex < 0 || clipIndex >= m_clips.size()) return;

    int oldIndex = m_currentClipIndex;
    m_currentClipIndex = clipIndex;

    const Clip& clip = m_clips[clipIndex];
    LOG_INFO("StreamSource", QString("switchToClip: deviceId=%1, clipIndex=%2, url=%3, start=%4, end=%5, offset=%6")
        .arg(m_deviceId).arg(clipIndex).arg(clip.url).arg(clip.startTimeMs).arg(clip.endTimeMs).arg(startOffsetMs));

    emit clipChanged(oldIndex, clipIndex);

    if (m_player) {
        m_player->loadFile(clip.url, startOffsetMs > 0 ? startOffsetMs : 0);
    }
}

void StreamSource::onPlayerStateChanged(ReplayPlayer::State newState) {
    switch (newState) {
        case ReplayPlayer::State::Playing:
            setState(State::Playing);
            break;
        case ReplayPlayer::State::Ready:
            break;
        case ReplayPlayer::State::Paused:
            setState(State::Paused);
            break;
        case ReplayPlayer::State::Completed:
            if (m_currentClipIndex >= 0 && m_currentClipIndex < m_clips.size() - 1) {
                switchToClip(m_currentClipIndex + 1, 0);
            } else {
                setState(State::Stopped);
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
    Q_UNUSED(durationMs)
    if (m_currentClipIndex >= 0 && m_currentClipIndex < m_clips.size()) {
        const Clip& clip = m_clips[m_currentClipIndex];
        m_baseTimeMs = clip.startTimeMs + currentMs;
        m_lastProgressMs = currentMs;
        emit playbackProgressChanged(m_baseTimeMs);
    }
}

void StreamSource::setState(State state) {
    if (m_state != state) {
        m_state = state;
        emit stateChanged(state);
    }
}

}
