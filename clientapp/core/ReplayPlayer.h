#pragma once

#include <QWidget>
#include <QLabel>
#include <QString>
#include <atomic>
#include <functional>
#include "CoreExport.h"
#include "IVideoControls.h"
#include "manager/player_manager.h"
#include "manager/media_player.h"
#include "manager/player_types.h"
#include "platform/native_window_handle.h"

namespace rustcam {

class VideoPlayerToolbar;

class RUSTCAM_CORE_API ReplayPlayer : public QWidget, public IVideoControls {
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
    /// 重建 bridge 播放器（原生窗口被 Qt 重建时必须重新绑定 HWND）
    bool createBridgePlayer();
    void startBridgePlayback();
    //void ensureBridgeWindow();
    void play();
    void pause();
    void stop() override;
    void seek(int64_t positionMs);
    void setSpeed(double speed);
    void setVolume(double volume);
    void setMute(bool mute);

    QWidget* controlWidget() override { return this; }
    void requestClose() override;
    void setRotation(int angle) override;
    void setMirror(bool horizontal, bool vertical) override;
    bool takeScreenshot(const QString& filePath) override;
    void setDeviceName(const QString& deviceName) override;
    QString deviceName() const override { return m_deviceName; }
    void setZoomEnabled(bool enabled) override;
    void setAudioEnabled(bool enabled) override;
    void toggleFullscreen() override;
    void setSelected(bool selected) override;

    State state() const { return m_state; }
    int64_t currentTimeMs() const { return m_currentTimeMs; }
    int64_t durationMs() const { return m_durationMs; }
    bool isPlaying() const override { return m_state == State::Playing; }
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
    void closeRequested();

protected:
    void mousePressEvent(QMouseEvent* event) override;
    void resizeEvent(QResizeEvent* event) override;
    void enterEvent(QEvent* event) override;
    void leaveEvent(QEvent* event) override;
    void wheelEvent(QWheelEvent* event) override;
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
    void updateToolbarGeometry();
    void applyVideoTransforms();

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
    int m_statsLogTicks = 0;
    /// 已经交给 bridge_player 的原生窗口句柄（Qt 重建窗口后需要重新绑定）
    quint64 m_bridgeWindowId = 0;
    int m_rebuildCount = 0;
    qint64 m_lastRebuildMs = 0;
    /// 播放倍速（新建 bridge 播放器后需要重新应用）
    double m_speed = 1.0;

    ProgressCallback m_progressCallback;
    StateChangeCallback m_stateChangeCallback;
    ErrorCallback m_errorCallback;
    CompletedCallback m_completedCallback;
    QLabel* m_placeholderLabel;
    class QTimer* m_positionTimer = nullptr;
    VideoPlayerToolbar* m_toolbar = nullptr;
    int m_rotationAngle = 0;
    bool m_mirrorH = false;
    bool m_mirrorV = false;
    bool m_zoomEnabled = false;
    double m_zoomScale = 1.0;
    bool m_audioEnabled = true;
    bool m_fullscreen = false;
    QString m_deviceName;
};

}
