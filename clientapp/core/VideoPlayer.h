#pragma once

#include <QWidget>
#include <QMutex>
#include <QLabel>
#include <atomic>
#include "CoreExport.h"
#include "manager/player_manager.h"
#include "manager/player.h"
#include "platform/native_window_handle.h"
#include "VideoPlayerToolbar.h"

namespace rustcam {

class RUSTCAM_CORE_API VideoPlayer : public QWidget {
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

    void play(const QString& url);
    void stop();
    void switchTarget(quint64 targetId);

    bool isPlaying() const { return m_state == State::Playing; }
    State state() const { return m_state; }
    QString currentUrl() const { return m_url; }
    quint64 playerId() const { return m_playerId; }
    quint64 targetId() const { return m_targetId; }

    BridgePlayer::PlayerBase::Ptr player() const { return m_player; }

    void setSelected(bool selected);

    void setZoomEnabled(bool enabled);
    void setZoom(float scale, int centerX, int centerY);
    void setRotation(int angle);
    void setMirror(bool horizontal, bool vertical);
    void setMirrorEnabled(bool enabled);
    bool takeScreenshot(const QString& filePath);
    void setWatermarkAccount(const QString& account);
    void setDeviceName(const QString& deviceName);
    void toggleFullscreen();
    void setAudioEnabled(bool enabled);
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
    void clicked();
    void fullscreenToggled(bool fullscreen);

protected:
    void mousePressEvent(QMouseEvent* event) override;
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
