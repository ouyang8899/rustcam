#include "VideoPlayer.h"
#include "ConfigKeys.h"
#include "SystemConfig.h"
#include "common/Logger.h"
#include <QDebug>
#include <QApplication>
#include <QPainter>
#include <QSvgRenderer>
#include <QStyle>
#include <QStyleOption>
#include <QPainter>
#include <QDateTime>
#include "SessionManager.h"
#include "SystemConfig.h"
namespace rustcam {

VideoPlayer::VideoPlayer(QWidget* parent)
    : QWidget(parent)
    , m_playerId(0)
    , m_targetId(0)
    , m_state(State::Idle)
    , m_placeholderLabel(nullptr)
    , m_statusLabel(nullptr)
    , m_errorLabel(nullptr)
    , m_toolbar(nullptr)
    , m_selected(false)
{
    setObjectName("VideoPlayer");

    m_placeholderLabel = new QLabel(this);
    m_placeholderLabel->setAlignment(Qt::AlignCenter);
    m_placeholderLabel->setObjectName("NoSignalIcon");
    QSvgRenderer renderer(QString(":/icons/video_placeholder.svg"));
    if (renderer.isValid()) {
        QPixmap pixmap(48, 48);
        pixmap.fill(Qt::transparent);
        QPainter painter(&pixmap);
        renderer.render(&painter);
        m_placeholderLabel->setPixmap(pixmap);
    }
    m_placeholderLabel->setFixedSize(48, 48);

    m_statusLabel = new QLabel(this);
    m_statusLabel->setAlignment(Qt::AlignCenter);
    m_statusLabel->setObjectName("StatusLabel");
    m_statusLabel->setStyleSheet("QLabel { background-color: rgba(0, 0, 0, 150); color: white; padding: 8px 16px; border-radius: 4px; font-size: 13px; }");
    m_statusLabel->hide();

    m_errorLabel = new QLabel(this);
    m_errorLabel->setAlignment(Qt::AlignCenter);
    m_errorLabel->setObjectName("ErrorLabel");
    m_errorLabel->setStyleSheet("QLabel { background-color: rgba(180, 50, 50, 200); color: white; padding: 12px 20px; border-radius: 4px; font-size: 13px; }");
    m_errorLabel->hide();

    m_toolbar = new VideoPlayerToolbar(this);
    m_toolbar->setVideoPlayer(this);
    m_toolbar->setFixedHeight(40);
    m_toolbar->hide();

    style()->polish(this);
    update();

    connect(SystemConfig::instance(), &SystemConfig::valueChanged,
            this, &VideoPlayer::onSystemConfigChanged);
}

VideoPlayer::~VideoPlayer() {
    stop();
}

void VideoPlayer::play(const QString& url) {
    QMutexLocker locker(&m_mutex);

    LOG_INFO("VideoPlayer", QString("play: url=%1, winId=%2").arg(url).arg((quint64)winId()));

    if (isPlaying()) {
        LOG_INFO("VideoPlayer", "play: was playing, stopping first");
        stop();
    }

    void* p = reinterpret_cast<void*>(winId());
    m_targetId = static_cast<quint64>(reinterpret_cast<quintptr>(p));
    m_url = url;

    BridgePlayer::NativeWindowHandle handle(reinterpret_cast<HWND>(winId()));
    LOG_INFO("VideoPlayer", QString("play: add_player(render_target_id=%1, player_id=0, hwnd=%2)")
        .arg(m_targetId).arg((quint64)handle.get_int_handle()));
    m_playerId = BridgePlayer::PlayerManager::instance().add_player(m_targetId, 0, handle);
    LOG_INFO("VideoPlayer", QString("play: add_player returned playerId=%1").arg(m_playerId));

    m_player = BridgePlayer::PlayerManager::instance().get_player(m_playerId);
    LOG_INFO("VideoPlayer", QString("play: get_player returned %1").arg(m_player ? "valid pointer" : "NULL"));

    if (!m_player) {
        LOG_ERROR("VideoPlayer", QString("play: failed to get player, playerId=%1").arg(m_playerId));
        emit error("Failed to create player");
        setState(State::Error);
        return;
    }

    m_player->set_play_status_callback([this](const BridgePlayer::PlayerBase::WPtr& player, BridgePlayer::PlayCode code) {
        Q_UNUSED(player);
        QMetaObject::invokeMethod(this, [this, code]() {
            onPlayerStatus(static_cast<int>(code));
        }, Qt::QueuedConnection);
    });

    setState(State::Connecting);
    //setUpdatesEnabled(false);
    emit connecting();

    auto activePlayers = BridgePlayer::PlayerManager::instance().get_active_players();
    bool contains = activePlayers.find(m_playerId) != activePlayers.end();
    LOG_INFO("VideoPlayer", QString("play: active players count=%1, contains our playerId=%2")
        .arg(activePlayers.size())
        .arg(contains ? "yes" : "NO"));

    LOG_INFO("VideoPlayer", QString("play: calling PlayerManager::play(playerId=%1, url=%2)").arg(m_playerId).arg(QString::fromStdString(url.toStdString())));
    bool success = BridgePlayer::PlayerManager::instance().play(m_playerId, url.toStdString());
    LOG_INFO("VideoPlayer", QString("play: PlayerManager::play returned %1").arg(success));

    if (!success) {
        LOG_ERROR("VideoPlayer", "play: PlayerManager::play failed, cleaning up");
        BridgePlayer::PlayerManager::instance().remove_player(m_playerId);
        m_playerId = 0;
        m_player.reset();
        emit error("Failed to start playback");
        setState(State::Error);
    }
}

void VideoPlayer::stop() {
    QMutexLocker locker(&m_mutex);

    if (m_playerId > 0) {
        BridgePlayer::PlayerManager::instance().stop(m_playerId);
        BridgePlayer::PlayerManager::instance().remove_player(m_playerId);
        m_playerId = 0;
        m_player.reset();
    }

    setState(State::Stopped);
    emit stopped();
}

void VideoPlayer::switchTarget(quint64 targetId) {
    QMutexLocker locker(&m_mutex);

    if (m_playerId == 0) return;

    m_targetId = targetId;
    if (m_player) {
        m_player->render_to_target(targetId);
    }
}

void VideoPlayer::setState(State state) {
    if (m_state != state) {
        m_state = state;
        emit stateChanged(state);
        updatePlaceholder();
        updateStatusLabel();
    }
}

void VideoPlayer::updatePlaceholder() {
    if (m_placeholderLabel) {
        bool show = (m_state == State::Idle || m_state == State::Stopped || m_state == State::Error);
        m_placeholderLabel->setVisible(show);
    }
}

void VideoPlayer::updateStatusLabel() {
    if (!m_statusLabel || !m_errorLabel) return;

    m_statusLabel->hide();
    m_errorLabel->hide();

    switch (m_state) {
        case State::Connecting:
            m_statusLabel->setText(tr("Connecting..."));
            m_statusLabel->show();
            break;
        case State::Connected:
            m_statusLabel->setText(tr("Starting..."));
            m_statusLabel->show();
            break;
        case State::Error:
            m_errorLabel->setText(tr("Connection failed"));
            m_errorLabel->show();
            break;
        default:
            break;
    }
}

void VideoPlayer::mousePressEvent(QMouseEvent* event) {
    Q_UNUSED(event)
    emit clicked();
}

void VideoPlayer::setSelected(bool selected) {
    if (m_selected != selected) {
        m_selected = selected;
    }
}

void VideoPlayer::resizeEvent(QResizeEvent* event) {
    QWidget::resizeEvent(event);

    if (m_placeholderLabel) {
        int x = (width() - m_placeholderLabel->width()) / 2;
        int y = (height() - m_placeholderLabel->height()) / 2;
        m_placeholderLabel->move(x, y);
    }

    if (m_statusLabel) {
        int labelW = 160;
        int labelH = 36;
        m_statusLabel->setFixedSize(labelW, labelH);
        int x = (width() - labelW) / 2;
        int y = (height() - labelH) / 2;
        m_statusLabel->move(x, y);
    }

    if (m_errorLabel) {
        int labelW = 200;
        int labelH = 48;
        m_errorLabel->setFixedSize(labelW, labelH);
        int x = (width() - labelW) / 2;
        int y = (height() - labelH) / 2;
        m_errorLabel->move(x, y);
    }

    if (m_toolbar) {
        m_toolbar->setGeometry(0, height() - m_toolbar->height(), width(), m_toolbar->height());
    }
}

void VideoPlayer::paintEvent(QPaintEvent* event) {
    QStyleOption opt;
    opt.init(this);
    QPainter p(this);
    style()->drawPrimitive(QStyle::PE_Widget, &opt, &p, this);
}

void VideoPlayer::enterEvent(QEvent* event) {
    QWidget::enterEvent(event);
    if (m_toolbar && isPlaying()) {
        m_toolbar->show();
    }
}

void VideoPlayer::leaveEvent(QEvent* event) {
    QWidget::leaveEvent(event);
    if (m_toolbar && !qApp->activePopupWidget()) {
        m_toolbar->hide();
    }
}

void VideoPlayer::wheelEvent(QWheelEvent* event) {
    if (m_toolbar) {
        QPointF globalPosF = event->globalPosition();
        QPoint globalPos(static_cast<int>(globalPosF.x()), static_cast<int>(globalPosF.y()));
        QPoint localPos = mapFromGlobal(globalPos);
        int delta = event->angleDelta().y();
        m_toolbar->handleWheelZoom(localPos.x(), localPos.y(), delta);
    }
    QWidget::wheelEvent(event);
}

void VideoPlayer::setZoomEnabled(bool enabled) {
    QMutexLocker locker(&m_mutex);
    if (m_player) {
        auto zoomData = std::make_shared<BridgePlayer::ZoomData>();
        zoomData->enable = enabled;
        m_player->set_zoom(zoomData);
    }
}

void VideoPlayer::setZoom(float scale, int centerX, int centerY) {
    QMutexLocker locker(&m_mutex);
    if (m_player) {
        auto zoomData = std::make_shared<BridgePlayer::ZoomData>();
        zoomData->scale = scale;
        zoomData->center = std::make_shared<BridgePlayer::Point2D>();
        zoomData->center->x = centerX;
        zoomData->center->y = centerY;
        zoomData->enable = true;
        m_player->set_zoom(zoomData);
    }
}

void VideoPlayer::setRotation(int angle) {
    QMutexLocker locker(&m_mutex);
    if (m_player) {
        m_player->set_roate_angle(angle);
    }
}

void VideoPlayer::setMirror(bool horizontal, bool vertical) {
    QMutexLocker locker(&m_mutex);
    if (m_player) {
        BridgePlayer::VideoMirrorMode mode = BridgePlayer::VideoMirrorMode::None;
        if (horizontal && vertical) {
            mode = BridgePlayer::VideoMirrorMode::Both;
        } else if (horizontal) {
            mode = BridgePlayer::VideoMirrorMode::Horizontal;
        } else if (vertical) {
            mode = BridgePlayer::VideoMirrorMode::Vertical;
        }
        m_player->set_mirror_mode(mode);
    }
}

void VideoPlayer::setMirrorEnabled(bool enabled) {
    QMutexLocker locker(&m_mutex);
    if (m_player) {
        if (!enabled) {
            m_player->set_mirror_mode(BridgePlayer::VideoMirrorMode::None);
        }
    }
}

bool VideoPlayer::takeScreenshot(const QString& filePath) {
    QMutexLocker locker(&m_mutex);
    if (m_player) {
        m_player->capture_video_image(filePath.toStdString());
        return true;
    }
    return false;
}

void VideoPlayer::onPlayerStatus(int code) {
    LOG_INFO("VideoPlayer", QString("onPlayerStatus: code=%1").arg(code));
    switch (static_cast<BridgePlayer::PlayCode>(code)) {
        case BridgePlayer::PlayCode::CONNECTING:
        case BridgePlayer::PlayCode::RE_CONNECT:
            setState(State::Connecting);
            if (m_placeholderLabel) {
                m_placeholderLabel->hide();
            }
            emit connecting();
            break;
        case BridgePlayer::PlayCode::CONNECTED:
            setState(State::Connected);
            if (m_placeholderLabel) {
                m_placeholderLabel->hide();
            }
           // setUpdatesEnabled(true);
            emit connected();
            break;
        case BridgePlayer::PlayCode::START_PLAY:
        case BridgePlayer::PlayCode::FIRST_FRAME:
            setState(State::Playing);
            if (m_placeholderLabel) {
                m_placeholderLabel->hide();
            }
            if (m_player) {
                applyOsdSettings();
            }
           // setUpdatesEnabled(false);
            emit playing();
            emit started();
            break;
        case BridgePlayer::PlayCode::DISCONNECT:
        case BridgePlayer::PlayCode::LOSE_SESSION:
            setState(State::Stopped);
            if (m_placeholderLabel) {
                m_placeholderLabel->show();
            }
            emit disconnected();
            emit stopped();
            break;
        case BridgePlayer::PlayCode::CANT_CONNECT:
            setState(State::Error);
            if (m_placeholderLabel) {
                m_placeholderLabel->show();
            }
            emit error(tr("Cannot connect to server"));
            emit stopped();
            break;
        case BridgePlayer::PlayCode::STOP_PLAY:
            setState(State::Stopped);
            //setUpdatesEnabled(true);
            if (m_placeholderLabel) {
                m_placeholderLabel->show();
            }
            emit stopped();
            break;
        case BridgePlayer::PlayCode::MEDIA_ERROR:
            setState(State::Error);
            //setUpdatesEnabled(true);
            if (m_placeholderLabel) {
                m_placeholderLabel->show();
            }
            emit error("Media error");
            emit stopped();
            break;
        default:
            break;
    }
}

void VideoPlayer::setWatermarkAccount(const QString& account) {
    m_watermarkAccount = account;
}

void VideoPlayer::setDeviceName(const QString& deviceName) {
    m_deviceName = deviceName;
}

void VideoPlayer::toggleFullscreen() {
    m_fullscreen = !m_fullscreen;
    if (m_fullscreen) {
        showFullScreen();
    } else {
        showNormal();
    }
    emit fullscreenToggled(m_fullscreen);
}

void VideoPlayer::setAudioEnabled(bool enabled) {
    m_audioEnabled = enabled;
    if (m_player) {
        m_player->set_audio_enabled(enabled);
    }
}

bool VideoPlayer::isAudioEnabled() const {
    return m_audioEnabled;
}

void VideoPlayer::setMediaRateVisible(bool visible) {
    if (m_player) {
        SystemConfig* cfg = SystemConfig::instance();

        auto convertAlign = [](int index) -> uint64_t {
            switch (index) {
                case 0: return static_cast<uint64_t>(BridgePlayer::Align::top | BridgePlayer::Align::left);
                case 1: return static_cast<uint64_t>(BridgePlayer::Align::top | BridgePlayer::Align::right);
                case 2: return static_cast<uint64_t>(BridgePlayer::Align::bottom | BridgePlayer::Align::left);
                case 3: return static_cast<uint64_t>(BridgePlayer::Align::bottom | BridgePlayer::Align::right);
                default: return static_cast<uint64_t>(BridgePlayer::Align::bottom | BridgePlayer::Align::right);
            }
        };

        QColor fontColor(cfg->getValue(CFG_OSD_FONT_COLOR).toString());

        auto rateData = std::make_shared<BridgePlayer::VideoMediaRateData>();
        rateData->color.r = fontColor.red();
        rateData->color.g = fontColor.green();
        rateData->color.b = fontColor.blue();
        rateData->color.a = cfg->getValue(CFG_OSD_BG_ALPHA).toInt();
        rateData->font_size = cfg->getValue(CFG_OSD_FONT_SIZE).toInt() - 3;
        rateData->align = convertAlign(cfg->getValue(CFG_MEDIA_RATE_ALIGN).toInt());
        rateData->mirror_roate_with_video = true;
        m_player->set_media_rate(rateData);
        m_player->set_media_rate_visible(visible);
    }
}

void VideoPlayer::applyOsdSettings() {
    if (!m_player) {
        return;
    }

    SystemConfig* cfg = SystemConfig::instance();

    auto convertAlign = [](int index) -> uint64_t {
        switch (index) {
            case 0: return static_cast<uint64_t>(BridgePlayer::Align::top | BridgePlayer::Align::left);
            case 1: return static_cast<uint64_t>(BridgePlayer::Align::top | BridgePlayer::Align::right);
            case 2: return static_cast<uint64_t>(BridgePlayer::Align::bottom | BridgePlayer::Align::left);
            case 3: return static_cast<uint64_t>(BridgePlayer::Align::bottom | BridgePlayer::Align::right);
            default: return static_cast<uint64_t>(BridgePlayer::Align::bottom | BridgePlayer::Align::right);
        }
    };

    auto setOsdStyle = [](auto& data, SystemConfig* cfg) {
        QColor fontColor(cfg->getValue(CFG_OSD_FONT_COLOR, "#FFFFFFFF").toString());
        data->color.r = fontColor.red();
        data->color.g = fontColor.green();
        data->color.b = fontColor.blue();
        data->color.a = cfg->getValue(CFG_OSD_BG_ALPHA).toInt();
        data->font_size = cfg->getValue(CFG_OSD_FONT_SIZE).toInt();
        data->mirror_roate_with_video = true;
    };

    if (cfg->getValue(CFG_DEVICE_NAME_VISIBLE).toBool() && !m_deviceName.isEmpty()) {
        auto nameData = std::make_shared<BridgePlayer::VideoNameData>();
        nameData->name = m_deviceName.toStdString();
        setOsdStyle(nameData, cfg);
        nameData->align = convertAlign(cfg->getValue(CFG_DEVICE_NAME_ALIGN).toInt());
        m_player->set_name(nameData);
        m_player->set_name_visible(true);
    } else {
        m_player->set_name_visible(false);
    }

    if (cfg->getValue(CFG_WATERMARK_VISIBLE).toBool() && !m_watermarkAccount.isEmpty()) {
        auto watermark = std::make_shared<BridgePlayer::VideoWatermarkData>();
        watermark->title = m_watermarkAccount.toStdString();
        setOsdStyle(watermark, cfg);
        watermark->degree = 30;
        m_player->set_watermark(watermark);
        m_player->set_watermark_visible(true);
    } else {
        m_player->set_watermark_visible(false);
    }

    {
        auto rateData = std::make_shared<BridgePlayer::VideoMediaRateData>();
        setOsdStyle(rateData, cfg);
        rateData->font_size = cfg->getValue(CFG_OSD_FONT_SIZE).toInt() - 3;
        rateData->align = convertAlign(cfg->getValue(CFG_MEDIA_RATE_ALIGN).toInt());
        m_player->set_media_rate(rateData);
        m_player->set_media_rate_visible(cfg->getValue(CFG_MEDIA_RATE_VISIBLE).toBool());
    }

    if (cfg->getValue(CFG_TIME_VISIBLE).toBool()) {
        auto timeData = std::make_shared<BridgePlayer::VideoLocaleTimeData>();
        timeData->get_time_fun = []() -> std::string {
            QString timeFormat = SystemConfig::instance()->getValue(CFG_TIME_FORMAT).toString();
            return QDateTime::currentDateTime().toString(timeFormat).toStdString();
        };
        setOsdStyle(timeData, cfg);
        timeData->align = convertAlign(cfg->getValue(CFG_TIME_ALIGN).toInt());
        m_player->set_locale_time(timeData);
        m_player->set_locale_time_visible(true);
    } else {
        m_player->set_locale_time_visible(false);
    }
}

void VideoPlayer::onSystemConfigChanged(const QString& key, const QVariant& value) {
    Q_UNUSED(value);

    static const QStringList osdKeys = {
        CFG_OSD_FONT_COLOR,
        CFG_OSD_BG_ALPHA,
        CFG_OSD_FONT_SIZE,
        CFG_DEVICE_NAME_VISIBLE,
        CFG_DEVICE_NAME_ALIGN,
        CFG_WATERMARK_VISIBLE,
        CFG_MEDIA_RATE_VISIBLE,
        CFG_MEDIA_RATE_ALIGN,
        CFG_TIME_VISIBLE,
        CFG_TIME_ALIGN,
        CFG_TIME_FORMAT,
    };

    if (osdKeys.contains(key)) {
        applyOsdSettings();
    }
}

}
