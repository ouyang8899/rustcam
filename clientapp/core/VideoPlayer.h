#pragma once

#include <QWidget>
#include <QMutex>
#include <QLabel>
#include <atomic>
#include "CoreExport.h"
#include "manager/player_manager.h"
#include "manager/player.h"
#include "platform/native_window_handle.h"

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

signals:
    void started();
    void stopped();
    void error(const QString& msg);
    void stateChanged(State state);
    void connecting();
    void connected();
    void playing();
    void disconnected();

protected:
    void resizeEvent(QResizeEvent* event) override;

private:
    void onPlayerStatus(int code);
    void setState(State state);
    void updatePlaceholder();

    quint64 m_playerId = 0;
    quint64 m_targetId = 0;
    State m_state = State::Idle;
    QString m_url;
    BridgePlayer::PlayerBase::Ptr m_player;
    QMutex m_mutex;
    QLabel* m_placeholderLabel;
};

}
