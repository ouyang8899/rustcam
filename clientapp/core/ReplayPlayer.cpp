#include "ReplayPlayer.h"
#include "VideoPlayerToolbar.h"
#include "common/Logger.h"
#include <QPainter>
#include <QStyle>
#include <QStyleOption>
#include <QSvgRenderer>
#include <QTimer>
#include <QPointer>
#include <QApplication>
#include <QCursor>
#include <QWheelEvent>
#include <QWindow>
#include <vector>

namespace rustcam {

namespace {

// bridge_player 的 MediaPlayer 析构会破坏栈，播放器实例只能常驻到进程结束，
// 这里用故意泄漏的容器持有所有创建过的实例，避免进程退出时触发析构。
std::vector<BridgePlayer::MediaPlayer::Ptr>& retainedPlayers() {
    static auto* players = new std::vector<BridgePlayer::MediaPlayer::Ptr>();
    return *players;
}

// bridge_player 的渲染目标使用较小的自增 ID（与 bridge_player 官方示例一致）
//std::atomic<quint64>& nextRenderTargetId() {
//    static auto* seq = new std::atomic<quint64>(1);
//    return *seq;
//}

}

ReplayPlayer::ReplayPlayer(QWidget* parent)
    : QWidget(parent)
    , m_playerId(0)
    , m_targetId(0)
    , m_state(State::Idle)
    , m_placeholderLabel(nullptr)
{
    setObjectName("ReplayPlayer");
    // 与 VideoPlayer 一致：不要自动填充背景，否则 Qt 绘制的背景会覆盖 bridge 的渲染画面
    setAutoFillBackground(false);

    // 立即创建原生窗口并固定句柄：bridge_player 需要一个稳定的 HWND 作为渲染目标，
    // 如果等到播放时才调用 winId()，控件在此期间被重新父级化会导致原生窗口被重建，
    // bridge 仍然会往已经销毁的 HWND 上渲染，表现为“解码正常但没有画面”。
    const WId nativeId = winId();
    LOG_INFO("ReplayPlayer",
             QString("native window created: hwnd=%1").arg(static_cast<qulonglong>(nativeId)));


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

    m_toolbar = new VideoPlayerToolbar(this);
    m_toolbar->setControls(this);
    m_toolbar->setCapabilities(true, true, true, false);
    // bridge 的回放渲染器直接向本控件的 HWND 呈现 D3D 画面，嵌在控件内部的
    // 子窗口会被渲染画面覆盖；这里把工具栏做独立置顶窗口，保证浮在视频之上。
    m_toolbar->setWindowFlags(Qt::Tool | Qt::FramelessWindowHint |
                              Qt::NoDropShadowWindowHint | Qt::WindowStaysOnTopHint);
    m_toolbar->setAttribute(Qt::WA_ShowWithoutActivating);
    m_toolbar->setFixedHeight(40);
    m_toolbar->hide();

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

    const int64_t startMs = startPositionMs > 0 ? startPositionMs : 0;
    m_url = url;
    m_currentTimeMs = startMs;
    m_durationMs = -1;
    m_lastPositionMs = -1;
    m_stallTicks = 0;
    m_completionSignalled = false;
    m_pendingStartMs = startMs;
    m_rebuildCount = 0;
    m_lastRebuildMs = 0;

    // 与 VideoPlayer 保持一致：先确保控件（及其原生窗口）可见，
    // 否则 bridge 会把画面渲染到隐藏的 HWND 上，表现为黑屏。
    if (!isVisible()) {
        show();
    }

    if (!createBridgePlayer()) {
        return;
    }
    startBridgePlayback();
}

bool ReplayPlayer::createBridgePlayer() {
    const uint64_t previousId = m_playerId.load();
    BridgePlayer::MediaPlayer::Ptr previousPlayer = m_player;
    if (previousId > 0) {
        BridgePlayer::PlayerManager::instance().stop(previousId);
    }

    const quint64 nativeWindowId = static_cast<quint64>(winId());
    m_bridgeWindowId = nativeWindowId;
    m_targetId = nativeWindowId;

    const int outputWidth = width() > 0 ? width() : 1280;
    const int outputHeight = height() > 0 ? height() : 720;

    BridgePlayer::NativeWindowHandle handle(reinterpret_cast<HWND>(winId()));

    const uint64_t playerId =
        BridgePlayer::PlayerManager::instance().add_media_player(m_targetId, 0, handle);
    m_playerId.store(playerId);
    m_player = std::static_pointer_cast<BridgePlayer::MediaPlayer>(
        BridgePlayer::PlayerManager::instance().get_player(playerId));

    if (!m_player) {
        reportError(QString("loadFile: failed to get player, playerId=%1").arg(playerId));
        return false;
    }

    m_player->set_audio_enabled(m_audioEnabled);
    BridgePlayer::PlayerManager::instance().set_player_output_size(
        playerId, outputWidth, outputHeight);
    LOG_INFO("ReplayPlayer",
             QString("loadFile: winId=%1 targetId=%2 size=%3x%4 visible=%5 playerId=%6")
                 .arg(nativeWindowId)
                 .arg(static_cast<qulonglong>(m_targetId))
                 .arg(outputWidth)
                 .arg(outputHeight)
                 .arg(isVisible() ? "true" : "false")
                 .arg(static_cast<qulonglong>(playerId)));

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

    applyVideoTransforms();

    return true;
}

void ReplayPlayer::startBridgePlayback() {
    if (!m_player) {
        return;
    }
    const uint64_t playerId = m_playerId.load();
    setState(State::Loading);
    auto player = BridgePlayer::PlayerManager::instance().get_player(playerId);
    if (!player) {
        return;
    }
    QSize size = this->size();
    BridgePlayer::PlayerManager::instance().set_player_output_size(playerId, size.width(), size.height());
    BridgePlayer::MediaPlayer* mp = dynamic_cast<BridgePlayer::MediaPlayer*>(player.get());
    if (!mp->play(m_url.toUtf8().toStdString(),0)) {
        reportError("startBridgePlayback: play rejected by player");
        return;
    }
    LOG_INFO("ReplayPlayer",
             QString("startBridgePlayback: hwnd=%1 targetId=%2 playerId=%3 current=%4 self=%5")
                 .arg(m_bridgeWindowId)
                 .arg(static_cast<qulonglong>(m_targetId))
                 .arg(static_cast<qulonglong>(playerId))
                 .arg(static_cast<qulonglong>(m_player->get_current_render_target()))
                 .arg(static_cast<qulonglong>(m_player->get_self_render_target())));
}

//void ReplayPlayer::ensureBridgeWindow() {
//    if (!m_player) {
//        return;
//    }
//    const quint64 currentWindowId = static_cast<quint64>(winId());
//    if (currentWindowId == m_bridgeWindowId) {
//        return;
//    }
//    // Qt 重建了原生窗口：只需重新绑定渲染目标，不需要重新解码
//    LOG_WARNING("ReplayPlayer",
//                QString("ensureBridgeWindow: native window changed %1 -> %2, rebind render target")
//                    .arg(m_bridgeWindowId)
//                    .arg(currentWindowId));
//    m_bridgeWindowId = currentWindowId;
//    m_targetId = currentWindowId;
//
//    BridgePlayer::NativeWindowHandle handle(reinterpret_cast<HWND>(winId()));
//    BridgePlayer::RenderTargetData rtd;
//    rtd.handle = handle;
//    rtd.target_id = m_targetId;
//    rtd.width = width() > 0 ? width() : 1280;
//    rtd.height = height() > 0 ? height() : 720;
//    BridgePlayer::PlayerManager::instance().build_render_target(rtd);
//    m_player->render_to_target(m_targetId);
//    BridgePlayer::PlayerManager::instance().set_player_output_size(
//        m_player->id(), rtd.width, rtd.height);
//}
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

void ReplayPlayer::requestClose() {
    stop();
    emit closeRequested();
}

void ReplayPlayer::setRotation(int angle) {
    m_rotationAngle = angle;
    if (m_player) {
        m_player->set_roate_angle(angle);
    }
}

void ReplayPlayer::setMirror(bool horizontal, bool vertical) {
    m_mirrorH = horizontal;
    m_mirrorV = vertical;
    applyVideoTransforms();
}

bool ReplayPlayer::takeScreenshot(const QString& filePath) {
    if (!m_player) {
        return false;
    }
    m_player->capture_video_image(filePath.toUtf8().toStdString());
    return true;
}

void ReplayPlayer::setDeviceName(const QString& deviceName) {
    m_deviceName = deviceName;
}

void ReplayPlayer::setZoomEnabled(bool enabled) {
    m_zoomEnabled = enabled;
    m_zoomScale = 1.0;
    applyVideoTransforms();
}

void ReplayPlayer::setAudioEnabled(bool enabled) {
    m_audioEnabled = enabled;
    if (m_player) {
        m_player->set_audio_enabled(enabled);
    }
}

void ReplayPlayer::toggleFullscreen() {
    m_fullscreen = !m_fullscreen;
    if (m_fullscreen) {
        showFullScreen();
    } else {
        showNormal();
    }
}

void ReplayPlayer::setSelected(bool selected) {
    Q_UNUSED(selected);
}

void ReplayPlayer::applyVideoTransforms() {
    if (!m_player) {
        return;
    }

    m_player->set_roate_angle(m_rotationAngle);

    if (m_mirrorH || m_mirrorV) {
        BridgePlayer::VideoMirrorMode mode = BridgePlayer::VideoMirrorMode::None;
        if (m_mirrorH && m_mirrorV) {
            mode = BridgePlayer::VideoMirrorMode::Both;
        } else if (m_mirrorH) {
            mode = BridgePlayer::VideoMirrorMode::Horizontal;
        } else {
            mode = BridgePlayer::VideoMirrorMode::Vertical;
        }
        m_player->set_mirror_mode(mode);
    } else {
        m_player->set_mirror_mode(BridgePlayer::VideoMirrorMode::None);
    }

    BridgePlayer::ZoomData::Ptr zoom = std::make_shared<BridgePlayer::ZoomData>();
    zoom->enable = m_zoomEnabled;
    zoom->scale = m_zoomScale;
    m_player->set_zoom(zoom);
}

void ReplayPlayer::wheelEvent(QWheelEvent* event) {
    if (!m_zoomEnabled || !m_player) {
        QWidget::wheelEvent(event);
        return;
    }
    const int delta = event->angleDelta().y();
    if (delta == 0) {
        QWidget::wheelEvent(event);
        return;
    }
    const double oldScale = m_zoomScale;
    if (delta > 0) {
        m_zoomScale *= 1.1;
    } else {
        m_zoomScale /= 1.1;
    }
    m_zoomScale = qBound(1.0, m_zoomScale, 10.0);
    if (qFuzzyCompare(oldScale, m_zoomScale)) {
        QWidget::wheelEvent(event);
        return;
    }
    const QPoint pos = event->position().toPoint();
    BridgePlayer::ZoomData::Ptr zoom = std::make_shared<BridgePlayer::ZoomData>();
    zoom->center = std::make_shared<BridgePlayer::Point2D>();
    zoom->center->x = pos.x();
    zoom->center->y = pos.y();
    zoom->scale = m_zoomScale;
    zoom->enable = true;
    m_player->set_zoom(zoom);
    event->accept();
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
    //ensureBridgeWindow();
    if (m_state != State::Playing && m_state != State::Buffering && m_state != State::Paused &&
        m_state != State::Ready && m_state != State::Seeking) {
        return;
    }
    if (m_durationMs <= 0) {
        updateMediaInfo();
    }
    if (++m_statsLogTicks >= 10) {
        m_statsLogTicks = 0;
        const auto& st = m_player->get_stats();
        LOG_INFO("ReplayPlayer",
                 QString("stats: decoded=%1 converted=%2 dropped=%3 rendered=%4 broadcast=%5 vq=%6 aq=%7 errors=%8")
                     .arg(st.frames_decoded.load())
                     .arg(st.frames_converted.load())
                     .arg(st.frames_dropped.load())
                     .arg(st.frames_rendererd.load())
                     .arg(st.frames_broadcast.load())
                     .arg(st.video_queue_size.load())
                     .arg(st.audio_frames_decoded.load())
                     .arg(st.errors.load()));
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
    if (!this->isVisible()) {
        puts("xx");
    }
    BridgePlayer::PlayCode pCode = static_cast<BridgePlayer::PlayCode>(code);
    switch (pCode) {
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

void ReplayPlayer::enterEvent(QEvent* event) {
    QWidget::enterEvent(event);
    if (m_toolbar && m_player) {
        m_toolbar->show();
        updateToolbarGeometry();
        m_toolbar->raise();
    }
}

void ReplayPlayer::leaveEvent(QEvent* event) {
    QWidget::leaveEvent(event);
    if (!m_toolbar || qApp->activePopupWidget()) {
        return;
    }
    // 工具栏是独立窗口，鼠标移入工具栏时本控件会收到 Leave，此时不能隐藏。
    if (!m_toolbar->frameGeometry().contains(QCursor::pos())) {
        m_toolbar->hide();
    }
}

void ReplayPlayer::updateToolbarGeometry() {
    if (!m_toolbar) {
        return;
    }
    // 工具栏是独立的顶层窗口：先把它的屏幕/DPR 对齐到播放器所在显示器，
    // 否则 Qt 会按主屏 DPR 换算，在混合 DPI 的多显示器上位置会偏移。
    QWindow* toolbarWindow = m_toolbar->windowHandle();
    QWindow* playerWindow = windowHandle();
    if (toolbarWindow && playerWindow) {
        toolbarWindow->setScreen(playerWindow->screen());
        const QPoint topLeft = mapToGlobal(QPoint(0, height() - m_toolbar->height()));
        toolbarWindow->setGeometry(
            QRect(topLeft, QSize(width(), m_toolbar->height())));
        return;
    }
    m_toolbar->setFixedWidth(width());
    m_toolbar->move(mapToGlobal(QPoint(0, height() - m_toolbar->height())));
}

void ReplayPlayer::resizeEvent(QResizeEvent* event) {
    QWidget::resizeEvent(event);
    if (m_player && width() > 0 && height() > 0) {
        BridgePlayer::PlayerManager::instance().set_player_output_size(
            m_player->id(), width(), height());
    }
    if (m_placeholderLabel) {
        int x = (width() - m_placeholderLabel->width()) / 2;
        int y = (height() - m_placeholderLabel->height()) / 2;
        m_placeholderLabel->move(x, y);
    }
    if (m_toolbar && m_toolbar->isVisible()) {
        updateToolbarGeometry();
    }
}

void ReplayPlayer::paintEvent(QPaintEvent* event) {
    // 播放中画面由 bridge 直接渲染到本窗口，Qt 不能再绘制背景覆盖它
    if (m_state == State::Loading || m_state == State::Ready ||
        m_state == State::Playing || m_state == State::Buffering) {
        return;
    }
    QStyleOption opt;
    opt.init(this);
    QPainter p(this);
    style()->drawPrimitive(QStyle::PE_Widget, &opt, &p, this);
}

}
