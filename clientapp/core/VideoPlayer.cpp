#include "VideoPlayer.h"
#include <QDebug>
#include <QApplication>
#include <QPainter>
#include <QSvgRenderer>

namespace rustcam {

VideoPlayer::VideoPlayer(QWidget* parent)
    : QWidget(parent)
    , m_playerId(0)
    , m_targetId(0)
    , m_state(State::Idle)
    , m_placeholderLabel(nullptr)
    , m_selected(false)
{
    setAttribute(Qt::WA_PaintOnScreen, true);
    setObjectName("VideoPlayer");
    setAttribute(Qt::WA_StyledBackground);

    m_placeholderLabel = new QLabel(this);
    m_placeholderLabel->setAlignment(Qt::AlignCenter);
    QSvgRenderer renderer(QString(":/icons/no_signal.svg"));
    if (renderer.isValid()) {
        QPixmap pixmap(96, 96);
        pixmap.fill(Qt::transparent);
        QPainter painter(&pixmap);
        renderer.render(&painter);
        m_placeholderLabel->setPixmap(pixmap);
    }
    m_placeholderLabel->setStyleSheet("QLabel { background-color: transparent; }");
    m_placeholderLabel->setGeometry(rect());
}

VideoPlayer::~VideoPlayer() {
    stop();
}

void VideoPlayer::play(const QString& url) {
    QMutexLocker locker(&m_mutex);

    if (isPlaying()) {
        stop();
    }

    void* p = reinterpret_cast<void*>(winId());
    m_targetId = static_cast<quint64>(reinterpret_cast<quintptr>(p));
    m_url = url;

    BridgePlayer::NativeWindowHandle handle(reinterpret_cast<HWND>(winId()));
    m_playerId = BridgePlayer::PlayerManager::instance().add_player(0, m_targetId, handle);
    m_player = BridgePlayer::PlayerManager::instance().get_player(m_playerId);

    if (!m_player) {
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
    emit connecting();

    bool success = BridgePlayer::PlayerManager::instance().play(m_playerId, url.toStdString());
    if (!success) {
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
    }
}

void VideoPlayer::updatePlaceholder() {
    if (m_placeholderLabel) {
        bool show = (m_state != State::Playing && m_state != State::Connecting && m_state != State::Connected);
        m_placeholderLabel->setVisible(show);
    }
}

void VideoPlayer::mousePressEvent(QMouseEvent* event) {
    Q_UNUSED(event)
    emit clicked();
}

void VideoPlayer::setSelected(bool selected) {
    if (m_selected != selected) {
        m_selected = selected;
        setStyleSheet(selected ?
            "QWidget { border: 2px solid #4a9eff; background-color: #1a1a1a; }" :
            "QWidget { border: none; background-color: #1a1a1a; }");
    }
}

void VideoPlayer::resizeEvent(QResizeEvent* event) {
    QWidget::resizeEvent(event);
    if (m_placeholderLabel) {
        m_placeholderLabel->setGeometry(rect());
    }
}

void VideoPlayer::onPlayerStatus(int code) {
    switch (static_cast<BridgePlayer::PlayCode>(code)) {
        case BridgePlayer::PlayCode::CONNECTING:
        case BridgePlayer::PlayCode::RE_CONNECT:
            setState(State::Connecting);
            emit connecting();
            break;
        case BridgePlayer::PlayCode::CONNECTED:
            setState(State::Connected);
            emit connected();
            break;
        case BridgePlayer::PlayCode::START_PLAY:
        case BridgePlayer::PlayCode::FIRST_FRAME:
            setState(State::Playing);
            emit playing();
            emit started();
            break;
        case BridgePlayer::PlayCode::DISCONNECT:
        case BridgePlayer::PlayCode::CANT_CONNECT:
        case BridgePlayer::PlayCode::LOSE_SESSION:
            setState(State::Stopped);
            emit disconnected();
            emit stopped();
            break;
        case BridgePlayer::PlayCode::STOP_PLAY:
            setState(State::Stopped);
            emit stopped();
            break;
        case BridgePlayer::PlayCode::MEDIA_ERROR:
            setState(State::Error);
            emit error("Media error");
            emit stopped();
            break;
        default:
            break;
    }
}

}
