#pragma once

#include <QWidget>
#include <QMutex>
#include <QLabel>
#include <QPoint>
#include <QVariantMap>
#include <atomic>
#include "CoreExport.h"
#include "IVideoControls.h"
#include "manager/player_manager.h"
#include "manager/player.h"
#include "platform/native_window_handle.h"
#include "VideoPlayerToolbar.h"

class QDragEnterEvent;
class QDragMoveEvent;
class QDropEvent;
class QMouseEvent;

namespace rustcam {

class RUSTCAM_CORE_API VideoPlayer : public QWidget, public IVideoControls {
    Q_OBJECT
public:
    enum class State {
        Idle,
        Connecting,
        Connected,
        Playing,
        Stopped,
        Error
    };
    Q_ENUM(State)

    explicit VideoPlayer(QWidget* parent = nullptr);
    ~VideoPlayer();

    /// 播放器的唯一标识（换格子也不变）
    QString uid() const { return m_uid; }

    void play(const QString& url);
    void stop() override;
    //void switchTarget(quint64 targetId);

    bool isPlaying() const override { return m_state == State::Playing; }
    State state() const { return m_state; }
    QString currentUrl() const { return m_url; }
    quint64 playerId() const { return m_playerId; }
    quint64 targetId() const { return m_targetId; }

    BridgePlayer::PlayerBase::Ptr player() const { return m_player; }

    QWidget* controlWidget() override { return this; }

    void setSelected(bool selected) override;

    void setZoomEnabled(bool enabled) override;
    void setZoom(float scale, int centerX, int centerY);
    void setRotation(int angle) override;
    void setMirror(bool horizontal, bool vertical) override;
    void setMirrorEnabled(bool enabled);
    bool takeScreenshot(const QString& filePath) override;
    void setWatermarkAccount(const QString& account) override;
    void setDeviceName(const QString& deviceName) override;
    QString deviceName() const override { return m_deviceName; }
    void toggleFullscreen() override;
    void setAudioEnabled(bool enabled) override;
    bool isAudioEnabled() const;
    void setMediaRateVisible(bool visible);

    signals:
    void started();
    void stopped();
    void error(const QString& msg);
    void stateChanged(State state);
    void connecting();
    void connected();
    void playing();
    void disconnected();
    void clicked(const QString& uid);
    void fullscreenToggled(bool fullscreen);
    /// 有播放器被拖到了本播放器所在的位置（targetUid 为本播放器的 uid，sourceUid 为被拖动播放器的 uid）
    void playerDropped(const QString& targetUid, const QString& sourceUid);
    /// 有设备树节点被拖到了本播放器上（targetUid 为本播放器的 uid）
    void deviceDropped(const QString& targetUid, const QVariantMap& meta);

protected:
    void mousePressEvent(QMouseEvent* event) override;
    void mouseMoveEvent(QMouseEvent* event) override;
    void dragEnterEvent(QDragEnterEvent* event) override;
    void dragMoveEvent(QDragMoveEvent* event) override;
    void dropEvent(QDropEvent* event) override;
    void resizeEvent(QResizeEvent* event) override;
    void paintEvent(QPaintEvent* event) override;
    void enterEvent(QEvent* event) override;
    void leaveEvent(QEvent* event) override;
    void wheelEvent(QWheelEvent* event) override;

private slots:
    void onSystemConfigChanged(const QString& key, const QVariant& value);

private:
    void onPlayerStatus(int code);
    void setState(State state);
    void updatePlaceholder();
    void updateStatusLabel();
    void applyOsdSettings();

    QString m_uid;
    quint64 m_playerId = 0;
    quint64 m_targetId = 0;
    State m_state = State::Idle;
    QString m_url;
    BridgePlayer::PlayerBase::Ptr m_player;
    QMutex m_mutex;
    QLabel* m_placeholderLabel;
    QLabel* m_statusLabel;
    QLabel* m_errorLabel;
    VideoPlayerToolbar* m_toolbar;
    bool m_selected = false;
    /// 按下位置（用于判断是否变成拖动）
    QPoint m_pressPos;
    bool m_pressed = false;
    QString m_watermarkAccount;
    QString m_deviceName;
    bool m_fullscreen = false;
    bool m_audioEnabled = true;
    double m_zoomScale = 1.0;
    const double m_minScale = 1.0;
    const double m_maxScale = 10.0;
    const double m_zoomStep = 1.1;
    bool m_zoomEnabled = false;
};

}
