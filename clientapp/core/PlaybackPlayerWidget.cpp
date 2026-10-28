#include "PlaybackPlayerWidget.h"
#include "ReplayPlayer.h"
#include "StreamSource.h"
#include "PlaybackController.h"
#include "common/Logger.h"
#include <QPainter>
#include <QStyle>
#include <QStyleOption>
#include <QMouseEvent>
#include <QDrag>
#include <QMimeData>
#include <QApplication>
#include <QUuid>

namespace rustcam {

namespace {
/// 拖动播放器时用的 mime 类型
const char* const kPlayerMimeType = "application/x-rustcam-player";
}

PlaybackPlayerWidget::PlaybackPlayerWidget(QWidget* parent)
    : QWidget(parent)
    , m_player(nullptr)
    , m_source(nullptr)
    , m_controller(nullptr)
    , m_group(nullptr)
    , m_noRecordingLabel(nullptr)
    , m_selected(false)
{
    setObjectName("PlaybackPlayer");
    setAutoFillBackground(true);

    // 播放器的唯一标识：换格子也不变
    m_uid = QUuid::createUuid().toString(QUuid::WithoutBraces);

    m_player = new ReplayPlayer(this);
    m_source = new StreamSource(QString(), m_player, this);

    // 画面控件盖住了本控件，点击由它转发；否则窗口无法被选中
    connect(m_player, &ReplayPlayer::clicked, this, &PlaybackPlayerWidget::onPlayerClicked);
    connect(m_player, &ReplayPlayer::closeRequested, this, &PlaybackPlayerWidget::closeRequested);
    // 画面控件会先吃掉鼠标事件，在这里记录按下位置，否则拖拽无法启动
    m_player->installEventFilter(this);

    connect(m_source, &StreamSource::playbackProgressChanged,
            this, &PlaybackPlayerWidget::onSourceProgress);
    connect(m_source, &StreamSource::noRecording, this, &PlaybackPlayerWidget::showNoRecording);

    m_noRecordingLabel = new QLabel(this);
    m_noRecordingLabel->setAlignment(Qt::AlignCenter);
    m_noRecordingLabel->setObjectName("NoRecordingLabel");
    m_noRecordingLabel->setVisible(false);

    style()->polish(this);
}

PlaybackPlayerWidget::~PlaybackPlayerWidget() {
    if (m_group && m_source) {
        m_group->removeCell(m_source);
    }
}

void PlaybackPlayerWidget::bindDevice(const QString& deviceId) {
    m_deviceId = deviceId;
    m_source->setClips(QList<Clip>());

    if (m_group) {
        m_group->removeCell(m_source);
    }

    LOG_INFO("PlaybackPlayerWidget", QString("bindDevice: deviceId=%1").arg(deviceId));
}

void PlaybackPlayerWidget::loadClips(const QList<Clip>& clips) {
    m_source->setClips(clips);
    LOG_INFO("PlaybackPlayerWidget", QString("loadClips: deviceId=%1, clipCount=%2")
        .arg(m_deviceId).arg(clips.size()));
}

void PlaybackPlayerWidget::setController(PlaybackController* controller) {
    m_controller = controller;
}

void PlaybackPlayerWidget::setGroup(PlaybackGroup* group) {
    if (m_group && m_source) {
        m_group->removeCell(m_source);
    }
    m_group = group;
    if (m_group && m_source) {
        m_group->addCell(m_source);
    }
}

void PlaybackPlayerWidget::setSelected(bool selected) {
    if (m_selected != selected) {
        m_selected = selected;
        setProperty("selected", selected);
        style()->unpolish(this);
        style()->polish(this);
        update();
    }
}

void PlaybackPlayerWidget::onPlayerClicked() {
    emit clicked(m_deviceId);
}

void PlaybackPlayerWidget::onSourceProgress(int64_t absoluteTimeMs) {
    emit playbackProgressChanged(absoluteTimeMs);
}

void PlaybackPlayerWidget::showNoRecording(int64_t timeMs) {
    QDateTime dt = QDateTime::fromMSecsSinceEpoch(timeMs);
    QString timeStr = dt.toString("yyyy-MM-dd hh:mm:ss");
    m_noRecordingLabel->setText(tr("No Recording\n%1").arg(timeStr));
    m_noRecordingLabel->setVisible(true);
    LOG_INFO("PlaybackPlayerWidget", QString("showNoRecording: time=%1").arg(timeStr));
}

bool PlaybackPlayerWidget::eventFilter(QObject* watched, QEvent* event) {
    if (watched == m_player) {
        switch (event->type()) {
        case QEvent::MouseButtonPress: {
            QMouseEvent* mouseEvent = static_cast<QMouseEvent*>(event);
            if (mouseEvent->button() == Qt::LeftButton) {
                m_pressPos = m_player->mapTo(this, mouseEvent->pos());
                m_pressed = true;
            }
            break;
        }
        case QEvent::MouseButtonRelease:
            m_pressed = false;
            break;
        case QEvent::MouseMove: {
            QMouseEvent* mouseEvent = static_cast<QMouseEvent*>(event);
            if (!m_pressed || !(mouseEvent->buttons() & Qt::LeftButton)) {
                break;
            }
            const QPoint pos = m_player->mapTo(this, mouseEvent->pos());
            if ((pos - m_pressPos).manhattanLength() >= QApplication::startDragDistance()) {
                m_pressed = false;
                startDrag();
            }
            break;
        }
        default:
            break;
        }
    }
    return QWidget::eventFilter(watched, event);
}

void PlaybackPlayerWidget::startDrag() {
    // 拖动播放器：在格子之间切换，通道绑定跟着播放器走
    QDrag* drag = new QDrag(this);
    QMimeData* mime = new QMimeData();
    mime->setData(kPlayerMimeType, m_uid.toUtf8());
    drag->setMimeData(mime);
    drag->exec(Qt::MoveAction);
}

void PlaybackPlayerWidget::mousePressEvent(QMouseEvent* event) {
    if (event->button() == Qt::LeftButton) {
        m_pressPos = event->pos();
        m_pressed = true;
    }
    emit clicked(m_deviceId);
}

void PlaybackPlayerWidget::mouseMoveEvent(QMouseEvent* event) {
    if (!m_pressed || !(event->buttons() & Qt::LeftButton)) {
        return;
    }
    if ((event->pos() - m_pressPos).manhattanLength() < QApplication::startDragDistance()) {
        return;
    }
    m_pressed = false;
    startDrag();
}

void PlaybackPlayerWidget::resizeEvent(QResizeEvent* event) {
    QWidget::resizeEvent(event);
    if (m_player) {
        m_player->setGeometry(0, 0, width(), height());
    }
    if (m_noRecordingLabel) {
        int labelW = 150;
        int labelH = 60;
        int x = (width() - labelW) / 2;
        int y = (height() - labelH) / 2;
        m_noRecordingLabel->setGeometry(x, y, labelW, labelH);
    }
}

}
