#pragma once

#include <QWidget>
#include <QLabel>
#include <QString>
#include <atomic>
#include <functional>
#include "CoreExport.h"
#include "manager/player_manager.h"
#include "manager/media_player.h"
#include "manager/player_types.h"
#include "platform/native_window_handle.h"

namespace rustcam {

class RUSTCAM_CORE_API ReplayPlayer : public QWidget {
    Q_OBJECT
public:
    enum class State {
        Idle,
        Loading,
        Ready,
        Playing,
        Paused,
        Seeking,
        Buffering,
        Error,
        Completed
    };
    Q_ENUM(State)

    explicit ReplayPlayer(QWidget* parent = nullptr);
    ~ReplayPlayer();

    void loadFile(const QString& url, int64_t startPositionMs = 0);
    void play();
    void pause();
    void stop();
    void seek(int64_t positionMs);
    void setSpeed(double speed);
    void setVolume(double volume);
    void setMute(bool mute);

    State state() const { return m_state; }
    int64_t currentTimeMs() const { return m_currentTimeMs; }
    int64_t durationMs() const { return m_durationMs; }
    bool isPlaying() const { return m_state == State::Playing; }
    QString currentUrl() const { return m_url; }
    uint64_t playerId() const { return m_playerId.load(); }

    using ProgressCallback = std::function<void(int64_t currentMs, int64_t durationMs)>;
    using StateChangeCallback = std::function<void(State oldState, State newState)>;
    using ErrorCallback = std::function<void(const QString& error)>;
    using CompletedCallback = std::function<void()>;

    void setProgressCallback(ProgressCallback cb);
    void setStateChangeCallback(StateChangeCallback cb);
    void setErrorCallback(ErrorCallback cb);
    void setCompletedCallback(CompletedCallback cb);

    bool canSeek() const;

signals:
    void stateChanged(State state);
    void progressChanged(int64_t currentMs, int64_t durationMs);
    void errorOccurred(const QString& msg);
    void completed();
    void clicked();

protected:
    void mousePressEvent(QMouseEvent* event) override;
    void resizeEvent(QResizeEvent* event) override;
    void paintEvent(QPaintEvent* event) override;

private:
    void setState(State state);
    void reportError(const QString& message);
    void pollPosition();
    void updateMediaInfo();
    void applyPendingStartPosition();
    void handlePlaybackFinished();
    void onPlayerStatus(int code);
    void onMediaPlayerEvent(BridgePlayer::MediaPlayerEvent event, const BridgePlayer::MediaPlayerStatus& status);
    void updatePlaceholder();

    std::atomic<uint64_t> m_playerId{0};
    uint64_t m_targetId = 0;
    State m_state = State::Idle;
    QString m_url;

    BridgePlayer::MediaPlayer::Ptr m_player;

    int64_t m_currentTimeMs = 0;
    int64_t m_durationMs = -1;
    int64_t m_pendingStartMs = 0;
    int64_t m_lastPositionMs = -1;
    int m_stallTicks = 0;
    bool m_completionSignalled = false;
    /// 播放倍速（新建 bridge 播放器后需要重新应用）
    double m_speed = 1.0;

    ProgressCallback m_progressCallback;
    StateChangeCallback m_stateChangeCallback;
    ErrorCallback m_errorCallback;
    CompletedCallback m_completedCallback;
    QLabel* m_placeholderLabel;
    class QTimer* m_positionTimer = nullptr;
};

}
