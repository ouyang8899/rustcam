#include "PlaybackPlugin.h"
#include "core/EventBus.h"
#include "core/ServerApi.h"
#include "common/Logger.h"
#include <QWidget>
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QGridLayout>
#include <QLabel>
#include <QPushButton>
#include <QSlider>
#include <QComboBox>
#include <QDateTimeEdit>
#include <QCalendarWidget>
#include <QToolBar>
#include <QFrame>

namespace rustcam {

bool PlaybackPlugin::initialize(EventBus* bus, QObject* cfg)
{
    Q_UNUSED(cfg)
    m_isPlaying = false;
    m_playbackSpeed = 1.0;
    m_playbackTime = QDateTime::currentDateTime();

    EventBus::instance()->subscribe("playback/request", this, SLOT(onPlaybackRequested(QString,QVariantMap)));

    LOG_INFO("PlaybackPlugin", "Initialized");
    return true;
}

void PlaybackPlugin::shutdown()
{
    EventBus::instance()->unsubscribeAll(this);
}

QWidget* PlaybackPlugin::createWorkspace(QWidget* parent)
{
    m_workspace = new QWidget(parent);
    m_workspace->setObjectName("workspaceArea");
    m_workspace->setStyleSheet(R"(
        QWidget#workspaceArea {
            background-color: #2b2b2b;
        }
    )");

    QVBoxLayout* layout = new QVBoxLayout(m_workspace);
    layout->setContentsMargins(8, 8, 8, 8);
    layout->setSpacing(8);

    QToolBar* toolbar = new QToolBar(m_workspace);
    toolbar->setStyleSheet(R"(
        QToolBar {
            background-color: #1e1e1e;
            border: none;
            spacing: 8px;
            padding: 4px;
        }
    )");

    QPushButton* playBtn = new QPushButton("▶", toolbar);
    playBtn->setCursor(Qt::PointingHandCursor);
    playBtn->setStyleSheet(R"(
        QPushButton {
            background-color: #0078D4;
            color: white;
            border: none;
            border-radius: 4px;
            font-size: 16px;
            min-width: 40px;
        }
        QPushButton:hover {
            background-color: #1084d8;
        }
    )");
    connect(playBtn, &QPushButton::clicked, this, &PlaybackPlugin::onPlay);
    toolbar->addWidget(playBtn);

    QPushButton* pauseBtn = new QPushButton("⏸", toolbar);
    pauseBtn->setCursor(Qt::PointingHandCursor);
    pauseBtn->setStyleSheet(R"(
        QPushButton {
            background-color: #3e3e3e;
            color: white;
            border: none;
            border-radius: 4px;
            font-size: 16px;
            min-width: 40px;
        }
        QPushButton:hover {
            background-color: #4e4e4e;
        }
    )");
    connect(pauseBtn, &QPushButton::clicked, this, &PlaybackPlugin::onPause);
    toolbar->addWidget(pauseBtn);

    QPushButton* stopBtn = new QPushButton("⏹", toolbar);
    stopBtn->setCursor(Qt::PointingHandCursor);
    stopBtn->setStyleSheet(R"(
        QPushButton {
            background-color: #3e3e3e;
            color: white;
            border: none;
            border-radius: 4px;
            font-size: 16px;
            min-width: 40px;
        }
        QPushButton:hover {
            background-color: #4e4e4e;
        }
    )");
    connect(stopBtn, &QPushButton::clicked, this, &PlaybackPlugin::onStop);
    toolbar->addWidget(stopBtn);

    toolbar->addSeparator();

    QLabel* speedLabel = new QLabel("速度:", toolbar);
    speedLabel->setStyleSheet("color: #e0e0e0;");
    toolbar->addWidget(speedLabel);

    QComboBox* speedCombo = new QComboBox(toolbar);
    speedCombo->addItems({"0.25x", "0.5x", "1x", "2x", "4x", "8x", "16x"});
    speedCombo->setCurrentText("1x");
    speedCombo->setStyleSheet(R"(
        QComboBox {
            background-color: #2b2b2b;
            border: 1px solid #3e3e3e;
            border-radius: 4px;
            padding: 4px 8px;
            color: #e0e0e0;
            min-width: 70px;
        }
    )");
    connect(speedCombo, &QComboBox::currentTextChanged, this, [this](const QString& text) {
        bool ok;
        double speed = text.replace("x", "").toDouble(&ok);
        if (ok) {
            onSpeedChanged(speed);
        }
    });
    toolbar->addWidget(speedCombo);

    toolbar->addSeparator();

    QLabel* timeLabel = new QLabel("时间:", toolbar);
    timeLabel->setStyleSheet("color: #e0e0e0;");
    toolbar->addWidget(timeLabel);

    QDateTimeEdit* timeEdit = new QDateTimeEdit(QDateTime::currentDateTime(), toolbar);
    timeEdit->setStyleSheet(R"(
        QDateTimeEdit {
            background-color: #2b2b2b;
            border: 1px solid #3e3e3e;
            border-radius: 4px;
            padding: 4px 8px;
            color: #e0e0e0;
        }
    )");
    connect(timeEdit, &QDateTimeEdit::dateTimeChanged, this, &PlaybackPlugin::onSeek);
    toolbar->addWidget(timeEdit);

    layout->addWidget(toolbar);

    QFrame* videoFrame = new QFrame(m_workspace);
    videoFrame->setStyleSheet(R"(
        QFrame {
            background-color: #000;
            border: 1px solid #3e3e3e;
            border-radius: 4px;
            min-height: 400px;
        }
    )");
    QVBoxLayout* videoLayout = new QVBoxLayout(videoFrame);
    videoLayout->setAlignment(Qt::AlignCenter);
    QLabel* placeholder = new QLabel("录像回放窗口", videoFrame);
    placeholder->setStyleSheet("color: #666; font-size: 16px;");
    placeholder->setAlignment(Qt::AlignCenter);
    videoLayout->addWidget(placeholder);
    layout->addWidget(videoFrame, 1);

    QFrame* timelineFrame = new QFrame(m_workspace);
    timelineFrame->setStyleSheet(R"(
        QFrame {
            background-color: #1e1e1e;
            border: 1px solid #3e3e3e;
            border-radius: 4px;
            min-height: 80px;
        }
    )");
    QHBoxLayout* timelineLayout = new QHBoxLayout(timelineFrame);

    QCalendarWidget* calendar = new QCalendarWidget(timelineFrame);
    calendar->setStyleSheet(R"(
        QCalendarWidget {
            background-color: #1e1e1e;
        }
        QCalendarWidget QToolButton {
            color: #e0e0e0;
        }
        QCalendarWidget QMenu {
            background-color: #2b2b2b;
            color: #e0e0e0;
        }
        QCalendarWidget QSpinBox {
            background-color: #2b2b2b;
            color: #e0e0e0;
        }
    )");
    timelineLayout->addWidget(calendar);

    QLabel* timelinePlaceholder = new QLabel("时间轴", timelineFrame);
    timelinePlaceholder->setAlignment(Qt::AlignCenter);
    timelinePlaceholder->setStyleSheet("color: #666;");
    timelineLayout->addWidget(timelinePlaceholder, 1);

    layout->addWidget(timelineFrame);

    return m_workspace;
}

void PlaybackPlugin::onPlaybackRequested(const QString& topic, const QVariantMap& payload)
{
    Q_UNUSED(topic)
    QString deviceId = payload["deviceId"].toString();
    QDateTime startTime = payload["startTime"].toDateTime();
    QDateTime endTime = payload["endTime"].toDateTime();

    if (deviceId.isEmpty()) return;

    m_currentDeviceId = deviceId;
    if (!startTime.isValid()) startTime = QDateTime::currentDateTime().addDays(-1);
    if (!endTime.isValid()) endTime = QDateTime::currentDateTime();

    loadRecordings(deviceId, startTime, endTime);

    LOG_INFO("PlaybackPlugin", QString("Playback requested: %1").arg(deviceId));
}

void PlaybackPlugin::onPlay()
{
    m_isPlaying = true;
    LOG_INFO("PlaybackPlugin", "Playback started");
}

void PlaybackPlugin::onPause()
{
    m_isPlaying = false;
    LOG_INFO("PlaybackPlugin", "Playback paused");
}

void PlaybackPlugin::onStop()
{
    m_isPlaying = false;
    m_playbackTime = QDateTime();
    LOG_INFO("PlaybackPlugin", "Playback stopped");
}

void PlaybackPlugin::onSpeedChanged(double speed)
{
    m_playbackSpeed = speed;
    LOG_INFO("PlaybackPlugin", QString("Speed changed: %1x").arg(speed));
}

void PlaybackPlugin::onSeek(const QDateTime& time)
{
    m_playbackTime = time;
    LOG_INFO("PlaybackPlugin", QString("Seek to: %1").arg(time.toString()));
}

void PlaybackPlugin::loadRecordings(const QString& deviceId, const QDateTime& start, const QDateTime& end)
{
    Q_UNUSED(deviceId)
    Q_UNUSED(start)
    Q_UNUSED(end)
}

}
