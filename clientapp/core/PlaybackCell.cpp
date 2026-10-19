#include "PlaybackCell.h"
#include "ReplayPlayer.h"
#include "StreamSource.h"
#include "PlaybackController.h"
#include "common/Logger.h"
#include <QPainter>
#include <QStyle>
#include <QStyleOption>

namespace rustcam {

PlaybackCell::PlaybackCell(QWidget* parent)
    : QWidget(parent)
    , m_player(nullptr)
    , m_source(nullptr)
    , m_controller(nullptr)
    , m_group(nullptr)
    , m_noRecordingLabel(nullptr)
    , m_selected(false)
{
    setObjectName("PlaybackCell");
    setAutoFillBackground(true);

    m_player = new ReplayPlayer(this);
    m_source = new StreamSource(QString(), this);

    m_source->setPlayer(m_player);

    m_source->setNoRecordingCallback([this](int64_t timeMs) {
        showNoRecording(timeMs);
    });

    m_noRecordingLabel = new QLabel(this);
    m_noRecordingLabel->setAlignment(Qt::AlignCenter);
    m_noRecordingLabel->setObjectName("NoRecordingLabel");
    m_noRecordingLabel->setVisible(false);
    m_noRecordingLabel->setStyleSheet(R"(
        QLabel {
            color: #888;
            font-size: 14px;
            background-color: rgba(0, 0, 0, 180);
            border-radius: 4px;
            padding: 8px 16px;
        }
    )");

    style()->polish(this);
}

PlaybackCell::~PlaybackCell() {
    if (m_group && m_source) {
        m_group->removeCell(m_source);
    }
}

void PlaybackCell::bindDevice(const QString& deviceId) {
    QString oldDeviceId = m_deviceId;
    m_deviceId = deviceId;
    m_source->setClips(QList<Clip>());

    if (m_group) {
        m_group->removeCell(m_source);
    }

    emit deviceBound(deviceId);
    LOG_INFO("PlaybackCell", QString("bindDevice: cellId=%1, deviceId=%2").arg(m_cellId).arg(deviceId));
}

void PlaybackCell::loadClips(const QList<Clip>& clips) {
    m_source->setClips(clips);
    LOG_INFO("PlaybackCell", QString("loadClips: cellId=%1, deviceId=%2, clipCount=%3")
        .arg(m_cellId).arg(m_deviceId).arg(clips.size()));
}

void PlaybackCell::setController(PlaybackController* controller) {
    m_controller = controller;
}

void PlaybackCell::setGroup(PlaybackGroup* group) {
    if (m_group && m_source) {
        m_group->removeCell(m_source);
    }
    m_group = group;
    if (m_group && m_source) {
        m_group->addCell(m_source);
    }
}

void PlaybackCell::setSelected(bool selected) {
    if (m_selected != selected) {
        m_selected = selected;
        setProperty("selected", selected);
        style()->unpolish(this);
        style()->polish(this);
        update();
    }
}

void PlaybackCell::showNoRecording(int64_t timeMs) {
    QDateTime dt = QDateTime::fromMSecsSinceEpoch(timeMs);
    QString timeStr = dt.toString("yyyy-MM-dd hh:mm:ss");
    m_noRecordingLabel->setText(tr("No Recording\n%1").arg(timeStr));
    m_noRecordingLabel->setVisible(true);
    LOG_INFO("PlaybackCell", QString("showNoRecording: cellId=%1, time=%2").arg(m_cellId).arg(timeStr));
}

void PlaybackCell::mousePressEvent(QMouseEvent* event) {
    Q_UNUSED(event)
    emit clicked();
}

void PlaybackCell::paintEvent(QPaintEvent* event) {
    QStyleOption opt;
    opt.init(this);
    QPainter p(this);
    style()->drawPrimitive(QStyle::PE_Widget, &opt, &p, this);
}

void PlaybackCell::resizeEvent(QResizeEvent* event) {
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
