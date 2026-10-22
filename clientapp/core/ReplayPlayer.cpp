#include "ReplayPlayer.h"
#include "common/Logger.h"
#include <QPainter>
#include <QStyle>
#include <QStyleOption>
#include <QSvgRenderer>

namespace rustcam {

ReplayPlayer::ReplayPlayer(QWidget* parent)
    : QWidget(parent)
    , m_playerId(0)
    , m_targetId(0)
    , m_state(State::Idle)
    , m_placeholderLabel(nullptr)
{
    setObjectName("ReplayPlayer");
    setAutoFillBackground(true);

    m_placeholderLabel = new QLabel(this);
    m_placeholderLabel->setAlignment(Qt::AlignCenter);
    m_placeholderLabel->setObjectName("ReplayPlaceholder");
    QSvgRenderer renderer(QString(":/icons/video_placeholder.svg"));
    if (renderer.isValid()) {
        QPixmap pixmap(48, 48);
        pixmap.fill(Qt::transparent);
        QPainter painter(&pixmap);
        renderer.render(&painter);
        m_placeholderLabel->setPixmap(pixmap);
    }
    m_placeholderLabel->setFixedSize(48, 48);

    style()->polish(this);
    update();
}

ReplayPlayer::~ReplayPlayer() {
    stop();
}

void ReplayPlayer::loadFile(const QString& url) {
    QMutexLocker locker(&m_mutex);

    if (m_playerId > 0) {
        stop();
    }

    m_url = url;
    void* p = reinterpret_cast<void*>(winId());
    m_targetId = static_cast<quint64>(reinterpret_cast<quintptr>(p));

    BridgePlayer::NativeWindowHandle handle(reinterpret_cast<HWND>(winId()));
    m_playerId = BridgePlayer::PlayerManager::instance().add_media_player(m_targetId, 0, handle);
    m_player = std::static_pointer_cast<BridgePlayer::MediaPlayer>(
        BridgePlayer::PlayerManager::instance().get_player(m_playerId));

    if (!m_player) {
        LOG_ERROR("ReplayPlayer", QString("loadFile: failed to get player, playerId=%1").arg(m_playerId));
        setState(State::Error);
        if (m_errorCallback) {
            m_errorCallback("Failed to create player");
        }
        return;
    }

    m_player->set_play_status_callback([this](const BridgePlayer::PlayerBase::WPtr& player, BridgePlayer::PlayCode code) {
        Q_UNUSED(player);
        QMetaObject::invokeMethod(this, [this, code]() {
            onPlayerStatus(static_cast<int>(code));
        }, Qt::QueuedConnection);
    });

    m_player->set_event_callback([this](BridgePlayer::MediaPlayerEvent event, const BridgePlayer::MediaPlayerStatus& status) {
        QMetaObject::invokeMethod(this, [this, event, status]() {
            onMediaPlayerEvent(event, status);
        }, Qt::QueuedConnection);
    });

    setState(State::Loading);
    BridgePlayer::PlayerManager::instance().play(m_playerId, url.toStdString());
}

void ReplayPlayer::play() {
    QMutexLocker locker(&m_mutex);
    if (m_player && m_state == State::Ready) {
        m_player->start("");
    }
}

void ReplayPlayer::pause() {
    QMutexLocker locker(&m_mutex);
    if (m_player) {
        m_player->pause();
    }
}

void ReplayPlayer::stop() {
    QMutexLocker locker(&m_mutex);

    if (m_playerId > 0) {
        BridgePlayer::PlayerManager::instance().stop(m_playerId);
        BridgePlayer::PlayerManager::instance().remove_player(m_playerId);
        m_playerId = 0;
        m_player.reset();
    }

    setState(State::Idle);
    m_currentTimeMs = 0;
    m_durationMs = -1;
}

void ReplayPlayer::seek(int64_t positionMs) {
    QMutexLocker locker(&m_mutex);
    if (m_player) {
        m_player->seek(positionMs);
    }
}

void ReplayPlayer::setSpeed(double speed) {
    QMutexLocker locker(&m_mutex);
    if (m_player) {
        m_player->set_speed(speed);
    }
}

void ReplayPlayer::setVolume(double volume) {
    QMutexLocker locker(&m_mutex);
    if (m_player) {
        m_player->set_volume(volume);
    }
}

void ReplayPlayer::setMute(bool mute) {
    QMutexLocker locker(&m_mutex);
    if (m_player) {
        m_player->set_mute(mute);
    }
}

bool ReplayPlayer::canSeek() const {
    QMutexLocker locker(&m_mutex);
    if (m_player) {
        return m_player->can_seek();
    }
    return false;
}

void ReplayPlayer::setProgressCallback(ProgressCallback cb) {
    m_progressCallback = std::move(cb);
}

void ReplayPlayer::setStateChangeCallback(StateChangeCallback cb) {
    m_stateChangeCallback = std::move(cb);
}

void ReplayPlayer::setErrorCallback(ErrorCallback cb) {
    m_errorCallback = std::move(cb);
}

void ReplayPlayer::setCompletedCallback(CompletedCallback cb) {
    m_completedCallback = std::move(cb);
}

void ReplayPlayer::setState(State state) {
    if (m_state != state) {
        State oldState = m_state;
        m_state = state;
        emit stateChanged(state);
        if (m_stateChangeCallback) {
            m_stateChangeCallback(oldState, state);
        }
        updatePlaceholder();
    }
}

void ReplayPlayer::onPlayerStatus(int code) {
    switch (static_cast<BridgePlayer::PlayCode>(code)) {
        case BridgePlayer::PlayCode::CONNECTING:
        case BridgePlayer::PlayCode::RE_CONNECT:
            setState(State::Loading);
            break;

        case BridgePlayer::PlayCode::CONNECTED:
            if (m_state == State::Loading) {
                setState(State::Ready);
            }
            break;

        case BridgePlayer::PlayCode::START_PLAY:
        case BridgePlayer::PlayCode::FIRST_FRAME:
            setState(State::Playing);
            break;

        case BridgePlayer::PlayCode::STOP_PLAY:
            setState(State::Idle);
            break;

        case BridgePlayer::PlayCode::DISCONNECT:
        case BridgePlayer::PlayCode::CANT_CONNECT:
        case BridgePlayer::PlayCode::LOSE_SESSION:
        case BridgePlayer::PlayCode::MEDIA_ERROR:
            setState(State::Error);
            if (m_errorCallback) {
                m_errorCallback("Playback error");
            }
            emit errorOccurred("Playback error");
            break;

        default:
            break;
    }
}

void ReplayPlayer::onMediaPlayerEvent(BridgePlayer::MediaPlayerEvent event, const BridgePlayer::MediaPlayerStatus& status) {
    switch (event) {
        case BridgePlayer::MediaPlayerEvent::STATE_CHANGED:
            break;
        case BridgePlayer::MediaPlayerEvent::PROGRESS_UPDATED:
            m_currentTimeMs = status.current_time_ms;
            m_durationMs = status.total_duration_ms;
            emit progressChanged(m_currentTimeMs, m_durationMs);
            if (m_progressCallback) {
                m_progressCallback(m_currentTimeMs, m_durationMs);
            }
            break;
        case BridgePlayer::MediaPlayerEvent::BUFFERING_START:
            setState(State::Buffering);
            break;
        case BridgePlayer::MediaPlayerEvent::BUFFERING_END:
            if (m_state == State::Buffering) {
                setState(State::Playing);
            }
            break;
        case BridgePlayer::MediaPlayerEvent::SEEK_COMPLETED:
            setState(State::Ready);
            break;
        case BridgePlayer::MediaPlayerEvent::PLAYBACK_COMPLETE:
            setState(State::Completed);
            if (m_completedCallback) {
                m_completedCallback();
            }
            emit completed();
            break;
        case BridgePlayer::MediaPlayerEvent::ERROR_OCCURRED:
            setState(State::Error);
            if (m_errorCallback) {
                m_errorCallback(QString::fromStdString(status.status_text));
            }
            emit errorOccurred(QString::fromStdString(status.status_text));
            break;
        default:
            break;
    }
}

void ReplayPlayer::updatePlaceholder() {
    if (m_placeholderLabel) {
        bool show = (m_state != State::Playing &&
                     m_state != State::Loading &&
                     m_state != State::Buffering &&
                     m_state != State::Ready);
        m_placeholderLabel->setVisible(show);
    }
}

void ReplayPlayer::mousePressEvent(QMouseEvent* event) {
    Q_UNUSED(event)
    emit clicked();
}

void ReplayPlayer::resizeEvent(QResizeEvent* event) {
    QWidget::resizeEvent(event);
    if (m_placeholderLabel) {
        int x = (width() - m_placeholderLabel->width()) / 2;
        int y = (height() - m_placeholderLabel->height()) / 2;
        m_placeholderLabel->move(x, y);
    }
}

void ReplayPlayer::paintEvent(QPaintEvent* event) {
    QStyleOption opt;
    opt.init(this);
    QPainter p(this);
    style()->drawPrimitive(QStyle::PE_Widget, &opt, &p, this);
}

}
