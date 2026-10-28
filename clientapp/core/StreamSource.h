#pragma once

#include <QObject>
#include <QString>
#include <QList>
#include <functional>
#include "CoreExport.h"
#include "ReplayPlayer.h"
#include "TimelineWidget.h"

namespace rustcam {

struct Clip {
    QString url;
    int64_t startTimeMs;
    int64_t endTimeMs;
    int64_t durationMs;
    SegmentType segmentType = SegmentType::Normal;
    int64_t alarmId = 0;

    Clip() : startTimeMs(0), endTimeMs(0), durationMs(0), segmentType(SegmentType::Normal), alarmId(0) {}
    Clip(const QString& u, int64_t start, int64_t end)
        : url(u), startTimeMs(start), endTimeMs(end), durationMs(end - start), segmentType(SegmentType::Normal), alarmId(0) {}
};

class RUSTCAM_CORE_API StreamSource : public QObject {
    Q_OBJECT
public:
    enum class State {
        Stopped,
        Playing,
        Paused,
        NoRecording
    };
    Q_ENUM(State)

    /// 播放器在构造时传入：连接在构造函数里建立，之后不再变动
    StreamSource(const QString& deviceId, ReplayPlayer* player, QObject* parent = nullptr);
    ~StreamSource() override;

    const QString& deviceId() const { return m_deviceId; }
    const QList<Clip>& clips() const { return m_clips; }
    State state() const { return m_state; }
    int currentClipIndex() const { return m_currentClipIndex; }
    int64_t baseTimeMs() const { return m_baseTimeMs; }
    ReplayPlayer* player() const { return m_player; }

    void setClips(const QList<Clip>& clips);
    /// 追加分片（加载更多时用，不打断当前播放）
    void appendClips(const QList<Clip>& clips);

    void play();
    void pause();
    void stop();
    void seekTo(int64_t absoluteTimeMs);
    void setSpeed(double speed);

    bool isInRange(int64_t absoluteTimeMs) const;
    int64_t totalDurationMs() const;

signals:
    void stateChanged(State state);
    void clipChanged(int oldIndex, int newIndex);
    void noRecording(int64_t timeMs);
    void playbackProgressChanged(int64_t absoluteTimeMs);
    void gapDetected(int64_t timeMs, int64_t gapMs);

private:
    void setState(State newState);
    /// 分片按开始时间升序
    void sortClips();
    int findClipAt(int64_t absoluteTimeMs) const;
    int findNearestClip(int64_t absoluteTimeMs) const;
    void switchToClip(int clipIndex, int64_t startOffsetMs = 0);
    void onPlayerStateChanged(ReplayPlayer::State newState);
    void onPlayerProgress(int64_t currentMs, int64_t durationMs);

    QString m_deviceId;
    QList<Clip> m_clips;
    int m_currentClipIndex = -1;
    int64_t m_baseTimeMs = 0;
    State m_state = State::Stopped;
    ReplayPlayer* m_player = nullptr;

    int64_t m_lastProgressMs = 0;
};

}
