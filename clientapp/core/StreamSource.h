#pragma once

#include <QObject>
#include <QString>
#include <QList>
#include <QMutex>
#include <functional>
#include "CoreExport.h"
#include "ReplayPlayer.h"

namespace rustcam {

struct Clip {
    QString url;
    int64_t startTimeMs;
    int64_t endTimeMs;
    int64_t durationMs;

    Clip() : startTimeMs(0), endTimeMs(0), durationMs(0) {}
    Clip(const QString& u, int64_t start, int64_t end)
        : url(u), startTimeMs(start), endTimeMs(end), durationMs(end - start) {}
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

    explicit StreamSource(const QString& deviceId, QObject* parent = nullptr);
    ~StreamSource();

    const QString& deviceId() const { return m_deviceId; }
    const QList<Clip>& clips() const { return m_clips; }
    State state() const { return m_state; }
    int currentClipIndex() const { return m_currentClipIndex; }
    int64_t baseTimeMs() const { return m_baseTimeMs; }

    void setClips(const QList<Clip>& clips);
    void setPlayer(ReplayPlayer* player);

    void play();
    void pause();
    void stop();
    void seekTo(int64_t absoluteTimeMs);
    void setSpeed(double speed);

    bool isInRange(int64_t absoluteTimeMs) const;
    int64_t totalDurationMs() const;

    using StateChangeCallback = std::function<void(State oldState, State newState)>;
    using NoRecordingCallback = std::function<void(int64_t timeMs)>;
    using ClipFinishedCallback = std::function<void()>;

    void setStateChangeCallback(StateChangeCallback cb);
    void setNoRecordingCallback(NoRecordingCallback cb);
    void setClipFinishedCallback(ClipFinishedCallback cb);

signals:
    void stateChanged(State state);
    void clipChanged(int oldIndex, int newIndex);
    void noRecording(int64_t timeMs);

private:
    void setState(State newState);
    int findClipAt(int64_t absoluteTimeMs) const;
    void switchToClip(int clipIndex);
    void onPlayerStateChanged(ReplayPlayer::State oldState, ReplayPlayer::State newState);
    void onPlayerProgress(int64_t currentMs, int64_t durationMs);
    void onPlayerCompleted();

    QString m_deviceId;
    QList<Clip> m_clips;
    int m_currentClipIndex = -1;
    int64_t m_baseTimeMs = 0;
    State m_state = State::Stopped;
    ReplayPlayer* m_player = nullptr;
    QMutex m_mutex;

    StateChangeCallback m_stateChangeCallback;
    NoRecordingCallback m_noRecordingCallback;
    ClipFinishedCallback m_clipFinishedCallback;

    int64_t m_lastProgressMs = 0;
};

}
