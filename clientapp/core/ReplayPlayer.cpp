#include "ReplayPlayer.h"
#include "common/Logger.h"
#include <QPainter>
#include <QStyle>
#include <QStyleOption>
#include <QSvgRenderer>
#include <QTimer>
#include <QPointer>
#include <vector>

namespace rustcam {

namespace {

// bridge_player 的 MediaPlayer 析构会破坏栈，播放器实例只能常驻到进程结束，
// 这里用故意泄漏的容器持有所有创建过的实例，避免进程退出时触发析构。
std::vector<BridgePlayer::MediaPlayer::Ptr>& retainedPlayers() {
    static auto* players = new std::vector<BridgePlayer::MediaPlayer::Ptr>();
    return *players;
}

}

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

    m_positionTimer = new QTimer(this);
    m_positionTimer->setInterval(200);
    connect(m_positionTimer, &QTimer::timeout, this, &ReplayPlayer::pollPosition);
    m_positionTimer->start();
}

ReplayPlayer::~ReplayPlayer() {
    stop();
    // 让还在队列里的 bridge 事件回调失效
    m_playerId.store(0);
    // bridge_player 的 MediaPlayer 析构会破坏栈：实例只能常驻到进程结束
    if (m_player) {
        retainedPlayers().push_back(m_player);
        m_player.reset();
    }
}

void ReplayPlayer::loadFile(const QString& url, int64_t startPositionMs) {

    const uint64_t previousId = m_playerId.load();
    BridgePlayer::MediaPlayer::Ptr previousPlayer = m_player;
    if (previousId > 0) {
        BridgePlayer::PlayerManager::instance().stop(previousId);
    }

    const int64_t startMs = startPositionMs > 0 ? startPositionMs : 0;
    m_url = url;
    m_currentTimeMs = startMs;
    m_durationMs = -1;
    m_lastPositionMs = -1;
    m_stallTicks = 0;
    m_completionSignalled = false;
    m_pendingStartMs = 0;

    void* p = reinterpret_cast<void*>(winId());
    m_targetId = static_cast<quint64>(reinterpret_cast<quintptr>(p));

    BridgePlayer::NativeWindowHandle handle(reinterpret_cast<HWND>(winId()));
    const uint64_t playerId =
        BridgePlayer::PlayerManager::instance().add_media_player(m_targetId, 0, handle);
    m_playerId.store(playerId);
    m_player = std::static_pointer_cast<BridgePlayer::MediaPlayer>(
        BridgePlayer::PlayerManager::instance().get_player(playerId));

    if (!m_player) {
        reportError(QString("loadFile: failed to get player, playerId=%1").arg(playerId));
        return;
    }

    // bridge_player 的 MediaPlayer 析构会破坏栈，上一个播放器只能常驻到进程结束
    if (previousPlayer) {
        retainedPlayers().push_back(previousPlayer);
    }

    // 回调里只持弱引用：播放器常驻后仍可能在 ReplayPlayer 销毁后触发事件
    QPointer<ReplayPlayer> self(this);
    m_player->set_play_status_callback([self, playerId](const BridgePlayer::PlayerBase::WPtr& player, BridgePlayer::PlayCode code) {
        Q_UNUSED(player);
        if (!self || playerId != self->m_playerId.load()) {
            return;
        }
        QMetaObject::invokeMethod(self.data(), [self, code]() {
            if (self) {
                self->onPlayerStatus(static_cast<int>(code));
            }
        }, Qt::QueuedConnection);
    });

    m_player->set_event_callback([self, playerId](BridgePlayer::MediaPlayerEvent event, const BridgePlayer::MediaPlayerStatus& status) {
        if (!self || playerId != self->m_playerId.load()) {
            return;
        }
        QMetaObject::invokeMethod(self.data(), [self, event, status]() {
            if (self) {
                self->onMediaPlayerEvent(event, status);
            }
        }, Qt::QueuedConnection);
    });

    // 新播放器默认 1x，把用户选的倍速补上
    if (m_speed != 1.0) {
        m_player->set_speed(m_speed);
    }

    setState(State::Loading);
    auto result = m_player->play(url.toStdString(), 0);
    if (result.has_error()) {
        reportError(QString("loadFile: play failed: %1")
                        .arg(QString::fromStdString(result.error().full_message())));
        return;
    }
    if (!result.value()) {
        reportError("loadFile: play rejected by player");
        return;
    }
    m_pendingStartMs = startMs;
}

void ReplayPlayer::play() {
    if (!m_player) {
        return;
    }

    if (m_state == State::Paused || m_state == State::Ready) {
        auto result = m_player->resume();
        if (result.has_error()) {
            reportError(QString("play: resume failed: %1")
                            .arg(QString::fromStdString(result.error().full_message())));
        } else {
            setState(State::Playing);
        }
    } else if (m_state == State::Completed && !m_url.isEmpty()) {
        setState(State::Loading);
        auto result = m_player->play(m_url.toStdString(), 0);
        if (result.has_error()) {
            reportError(QString("play: restart failed: %1")
                            .arg(QString::fromStdString(result.error().full_message())));
        } else if (!result.value()) {
            reportError("play: restart rejected by player");
        }
    }
}

void ReplayPlayer::pause() {
    if (m_player) {
        auto position = m_player->position_ms();
        if (position.has_value()) {
            m_currentTimeMs = position.value();
        }
        auto result = m_player->pause();
        if (result.has_error()) {
            LOG_WARNING("ReplayPlayer", QString("pause failed: %1")
                                            .arg(QString::fromStdString(result.error().full_message())));
        }
        if (m_state == State::Playing || m_state == State::Buffering || m_state == State::Ready) {
            setState(State::Paused);
        }
    }
}

void ReplayPlayer::stop() {

    const uint64_t playerId = m_playerId.load();
    if (playerId > 0) {
        BridgePlayer::PlayerManager::instance().stop(playerId);
    }

    setState(State::Idle);
    m_currentTimeMs = 0;
    m_durationMs = -1;
}

void ReplayPlayer::seek(int64_t positionMs) {
    if (m_player) {
        auto result = m_player->seek_when_ready(positionMs);
        if (result.has_error()) {
            LOG_WARNING("ReplayPlayer", QString("seek failed: %1")
                                            .arg(QString::fromStdString(result.error().full_message())));
        }
    }
}

void ReplayPlayer::setSpeed(double speed) {
    m_speed = speed;
    if (m_player) {
        m_player->set_speed(speed);
    }
}

void ReplayPlayer::setVolume(double volume) {
    if (m_player) {
        m_player->set_volume(volume);
    }
}

void ReplayPlayer::setMute(bool mute) {
    if (m_player) {
        m_player->set_mute(mute);
    }
}

bool ReplayPlayer::canSeek() const {
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

void ReplayPlayer::pollPosition() {
    if (!m_player) {
        return;
    }
    if (m_state != State::Playing && m_state != State::Buffering && m_state != State::Paused &&
        m_state != State::Ready && m_state != State::Seeking) {
        return;
    }
    if (m_durationMs <= 0) {
        updateMediaInfo();
    }
    auto position = m_player->position_ms();
    if (!position.has_value()) {
        return;
    }
    const int64_t currentMs = position.value();
    if (currentMs < 0) {
        return;
    }
    if (m_durationMs > 0) {
        if (currentMs >= m_durationMs) {
            handlePlaybackFinished();
            return;
        }
    } else {
        // 时长未知时用「播放中位置连续多次不前进」判断已播完
        if (m_state == State::Playing) {
            if (currentMs == m_lastPositionMs && m_lastPositionMs > 0) {
                ++m_stallTicks;
            } else {
                m_stallTicks = 0;
                m_lastPositionMs = currentMs;
            }
            if (m_stallTicks >= 15) {
                handlePlaybackFinished();
                return;
            }
        } else {
            m_stallTicks = 0;
            m_lastPositionMs = currentMs;
        }
    }
    if (currentMs == m_currentTimeMs) {
        return;
    }
    m_currentTimeMs = currentMs;
    emit progressChanged(m_currentTimeMs, m_durationMs);
    if (m_progressCallback) {
        m_progressCallback(m_currentTimeMs, m_durationMs);
    }
}

void ReplayPlayer::handlePlaybackFinished() {
    if (m_completionSignalled) {
        return;
    }
    m_completionSignalled = true;
    if (m_durationMs > 0 && m_currentTimeMs != m_durationMs) {
        m_currentTimeMs = m_durationMs;
        emit progressChanged(m_currentTimeMs, m_durationMs);
        if (m_progressCallback) {
            m_progressCallback(m_currentTimeMs, m_durationMs);
        }
    }
    if (m_player) {
        m_player->pause();
    }
    setState(State::Completed);
    if (m_completedCallback) {
        m_completedCallback();
    }
    emit completed();
}

void ReplayPlayer::applyPendingStartPosition() {
    if (m_pendingStartMs <= 0 || !m_player) {
        return;
    }
    const int64_t startMs = m_pendingStartMs;
    m_pendingStartMs = 0;
    auto seekResult = m_player->seek_when_ready(startMs);
    if (seekResult.has_error()) {
        LOG_WARNING("ReplayPlayer", QString("applyPendingStartPosition: seek to %1ms failed: %2")
                                        .arg(startMs)
                                        .arg(QString::fromStdString(seekResult.error().full_message())));
    }
}

void ReplayPlayer::updateMediaInfo() {
    if (!m_player) {
        return;
    }
    auto info = m_player->get_media_info();
    if (!info) {
        return;
    }
    if (info->duration_ms > 0 && info->duration_ms != m_durationMs) {
        m_durationMs = info->duration_ms;
    }
}

void ReplayPlayer::reportError(const QString& message) {
    LOG_ERROR("ReplayPlayer", message);
    setState(State::Error);
    if (m_errorCallback) {
        m_errorCallback(message);
    }
    emit errorOccurred(message);
}

void ReplayPlayer::onPlayerStatus(int code) {
    switch (static_cast<BridgePlayer::PlayCode>(code)) {
        case BridgePlayer::PlayCode::CONNECTING:
        case BridgePlayer::PlayCode::RE_CONNECT:
            setState(State::Loading);
            break;

        case BridgePlayer::PlayCode::CONNECTED:
            updateMediaInfo();
            if (m_state == State::Loading) {
                setState(State::Ready);
            }
            applyPendingStartPosition();
            break;

        case BridgePlayer::PlayCode::START_PLAY:
        case BridgePlayer::PlayCode::FIRST_FRAME:
            if (m_state != State::Idle) {
                setState(State::Playing);
                applyPendingStartPosition();
            }
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
    if (m_state == State::Idle) {
        return;
    }
    switch (event) {
        case BridgePlayer::MediaPlayerEvent::STATE_CHANGED:
            break;
        case BridgePlayer::MediaPlayerEvent::PROGRESS_UPDATED:
            if (status.total_duration_ms > 0) {
                m_durationMs = status.total_duration_ms;
            }
            m_currentTimeMs = (m_durationMs > 0 && status.current_time_ms > m_durationMs)
                                  ? m_durationMs
                                  : status.current_time_ms;
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
            handlePlaybackFinished();
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
