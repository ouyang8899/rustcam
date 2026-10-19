#include "PlaybackPlugin.h"
#include "core/EventBus.h"
#include "core/ServerApi.h"
#include "core/ReplayPlayerContainer.h"
#include "core/PlaybackSidePanel.h"
#include "core/PlaybackController.h"
#include "core/PlaybackCell.h"
#include "core/RecordingFileListWidget.h"
#include "core/StreamSource.h"
#include "core/DataManager.h"
#include "common/Logger.h"
#include <QWidget>
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QPushButton>
#include <QComboBox>
#include <QLabel>
#include <QDateTimeEdit>
#include <QTimer>
#include <QSplitter>
#include <QToolBar>
#include <QJsonDocument>
#include <QtConcurrent>
#include <unordered_map>
#include <QFutureWatcher>

namespace rustcam {

bool PlaybackPlugin::initialize(EventBus* bus, QObject* cfg)
{
    Q_UNUSED(cfg)
    m_playbackSpeed = 1.0;
    m_playbackTime = QDateTime::currentDateTime();
    m_sidePanel = nullptr;
    m_container = nullptr;
    m_currentDeviceId = "";

    m_playbackRequestToken = EventBus::instance()->subscribe("playback/request", [this](const QString& topic, EventDataPtr data) {
        Q_UNUSED(data)
        onPlaybackRequested(topic, QVariantMap());
    });

    LOG_INFO("PlaybackPlugin", "Initialized");
    return true;
}

void PlaybackPlugin::shutdown()
{
    EventBus::instance()->unsubscribe(m_playbackRequestToken);
}

QWidget* PlaybackPlugin::createWorkspace(QWidget* parent)
{
    QWidget* workspace = new QWidget(parent);
    workspace->setObjectName("playbackWorkspace");
    workspace->setStyleSheet(R"(
        QWidget#playbackWorkspace {
            background-color: #2b2b2b;
        }
    )");

    QHBoxLayout* mainLayout = new QHBoxLayout(workspace);
    mainLayout->setContentsMargins(0, 0, 0, 0);
    mainLayout->setSpacing(0);

    m_sidePanel = new PlaybackSidePanel(workspace);
    mainLayout->addWidget(m_sidePanel);

    QWidget* rightPanel = new QWidget(workspace);
    rightPanel->setStyleSheet("background-color: #2b2b2b;");
    QVBoxLayout* rightLayout = new QVBoxLayout(rightPanel);
    rightLayout->setContentsMargins(4, 4, 4, 4);
    rightLayout->setSpacing(4);

    QToolBar* toolbar = new QToolBar(rightPanel);
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
    playBtn->setProperty("class", "ToolbarButton");
    playBtn->setFixedSize(36, 32);
    connect(playBtn, &QPushButton::clicked, this, &PlaybackPlugin::onPlay);
    toolbar->addWidget(playBtn);

    QPushButton* pauseBtn = new QPushButton("⏸", toolbar);
    pauseBtn->setCursor(Qt::PointingHandCursor);
    pauseBtn->setProperty("class", "ToolbarButton");
    pauseBtn->setFixedSize(36, 32);
    connect(pauseBtn, &QPushButton::clicked, this, &PlaybackPlugin::onPause);
    toolbar->addWidget(pauseBtn);

    QPushButton* stopBtn = new QPushButton("⏹", toolbar);
    stopBtn->setCursor(Qt::PointingHandCursor);
    stopBtn->setProperty("class", "ToolbarButton");
    stopBtn->setFixedSize(36, 32);
    connect(stopBtn, &QPushButton::clicked, this, &PlaybackPlugin::onStop);
    toolbar->addWidget(stopBtn);

    toolbar->addSeparator();

    QLabel* speedLabel = new QLabel(tr("Speed:"), toolbar);
    speedLabel->setStyleSheet("color: #e0e0e0;");
    toolbar->addWidget(speedLabel);

    QComboBox* speedCombo = new QComboBox(toolbar);
    speedCombo->addItems({"0.25x", "0.5x", "1x", "2x", "4x", "8x", "16x"});
    speedCombo->setCurrentText("1x");
    speedCombo->setProperty("class", "ToolbarComboBox");
    speedCombo->setFixedWidth(70);
    connect(speedCombo, &QComboBox::currentTextChanged, this, [this](const QString& text) {
        bool ok;
        double speed = text.left(text.size() - 1).toDouble(&ok);
        if (ok) {
            onSpeedChanged(speed);
        }
    });
    toolbar->addWidget(speedCombo);

    toolbar->addSeparator();

    QLabel* timeLabel = new QLabel(tr("Time:"), toolbar);
    timeLabel->setStyleSheet("color: #e0e0e0;");
    toolbar->addWidget(timeLabel);

    QDateTimeEdit* timeEdit = new QDateTimeEdit(m_playbackTime, toolbar);
    timeEdit->setProperty("class", "ToolbarDateTimeEdit");
    timeEdit->setCalendarPopup(true);
    timeEdit->setFixedWidth(160);
    connect(timeEdit, &QDateTimeEdit::dateTimeChanged, this, [this](const QDateTime& dt) {
        m_playbackTime = dt;
        onSeek(dt.toMSecsSinceEpoch());
    });
    toolbar->addWidget(timeEdit);

    QLabel* currentTimeLabel = new QLabel("--:--:--", toolbar);
    currentTimeLabel->setObjectName("currentTimeLabel");
    currentTimeLabel->setStyleSheet("color: #e0e0e0; font-family: monospace; min-width: 150px;");
    toolbar->addWidget(currentTimeLabel);

    QWidget* spacer = new QWidget(toolbar);
    spacer->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Preferred);
    toolbar->addWidget(spacer);

    rightLayout->addWidget(toolbar);

    m_container = new ReplayPlayerContainer(rightPanel);
    connect(m_container, &ReplayPlayerContainer::cellClicked, this, &PlaybackPlugin::onCellClicked);
    rightLayout->addWidget(m_container, 1);

    m_container->initialize();

    mainLayout->addWidget(rightPanel, 1);

    QTimer* updateTimer = new QTimer(workspace);
    connect(updateTimer, &QTimer::timeout, this, [this, currentTimeLabel]() {
        if (m_container && m_container->controller()) {
            int64_t baseTime = m_container->controller()->baseTimeMs();
            if (baseTime > 0) {
                QDateTime dt = QDateTime::fromMSecsSinceEpoch(baseTime);
                currentTimeLabel->setText(dt.toString("yyyy-MM-dd hh:mm:ss"));
            }
        }
    });
    updateTimer->start(1000);

    QTimer::singleShot(100, this, &PlaybackPlugin::onLoadDeviceTree);

    return workspace;
}

void PlaybackPlugin::onLoadDeviceTree() {
    if (DataManager::instance()->isLoaded()) {
        LOG_INFO("PlaybackPlugin", "Using DataManager data");
        onDataManagerLoaded();
        return;
    }

    LOG_INFO("PlaybackPlugin", "DataManager not loaded yet, loading...");
    connect(DataManager::instance(), &DataManager::loadFinished, this, &PlaybackPlugin::onDataManagerLoaded, Qt::UniqueConnection);
    DataManager::instance()->loadAll();
}

void PlaybackPlugin::onDataManagerLoaded()
{
    disconnect(DataManager::instance(), &DataManager::loadFinished, this, &PlaybackPlugin::onDataManagerLoaded);

    if (!DataManager::instance()->isLoaded()) {
        LOG_ERROR("PlaybackPlugin", "DataManager load failed");
        return;
    }

    if (m_sidePanel) {
        m_sidePanel->setRegionTreeNodes(DataManager::instance()->regionTree());
        m_sidePanel->setGroupTreeNodes(DataManager::instance()->groupTree());
        m_sidePanel->setDeviceTreeNodes(DataManager::instance()->deviceList());
    }

    LOG_INFO("PlaybackPlugin", QString("Using DataManager: regionNodes=%1, groupNodes=%2, deviceNodes=%3")
        .arg(DataManager::instance()->regionTree().size())
        .arg(DataManager::instance()->groupTree().size())
        .arg(DataManager::instance()->deviceList().size()));
}

void PlaybackPlugin::onPlaybackRequested(const QString& topic, const QVariantMap& payload)
{
    Q_UNUSED(topic)
    QString deviceId = payload["deviceId"].toString();
    QDateTime startTime = payload["startTime"].toDateTime();
    QDateTime endTime = payload["endTime"].toDateTime();

    if (deviceId.isEmpty()) return;

    if (!startTime.isValid()) startTime = QDateTime::currentDateTime().addDays(-1);
    if (!endTime.isValid()) endTime = QDateTime::currentDateTime();

    m_playbackTime = startTime;
    onDeviceSelected(deviceId, deviceId);

    LOG_INFO("PlaybackPlugin", QString("Playback requested: %1").arg(deviceId));
}

void PlaybackPlugin::onPlay()
{
    if (m_container) {
        m_container->play();
    }
    LOG_INFO("PlaybackPlugin", "Playback started");
}

void PlaybackPlugin::onPause()
{
    if (m_container) {
        m_container->pause();
    }
    LOG_INFO("PlaybackPlugin", "Playback paused");
}

void PlaybackPlugin::onStop()
{
    if (m_container) {
        m_container->stop();
    }
    m_playbackTime = QDateTime();
    LOG_INFO("PlaybackPlugin", "Playback stopped");
}

void PlaybackPlugin::onSpeedChanged(double speed)
{
    m_playbackSpeed = speed;
    if (m_container) {
        m_container->setSpeed(speed);
    }
    LOG_INFO("PlaybackPlugin", QString("Speed changed: %1x").arg(speed));
}

void PlaybackPlugin::onSeek(int64_t timeMs)
{
    if (m_container) {
        m_container->setBaseTime(timeMs);
    }
    LOG_INFO("PlaybackPlugin", QString("Seek to: %1").arg(QDateTime::fromMSecsSinceEpoch(timeMs).toString()));
}

void PlaybackPlugin::onCellClicked(int cellId, const QString& deviceId)
{
    Q_UNUSED(cellId)
    Q_UNUSED(deviceId)
}

void PlaybackPlugin::onDeviceSelected(const QString& deviceId, const QString& deviceName)
{
    Q_UNUSED(deviceName)
    m_currentDeviceId = deviceId;
    if (m_sidePanel) {
        m_sidePanel->setSelectedDate(QDate::currentDate());
    }
    LOG_INFO("PlaybackPlugin", QString("Device selected: %1").arg(deviceId));
}

void PlaybackPlugin::onTimeSelected(int64_t timeMs)
{
    onSeek(timeMs);
}

void PlaybackPlugin::onDateChanged(const QDate& date)
{
    Q_UNUSED(date)
    if (!m_currentDeviceId.isEmpty() && m_sidePanel) {
    }
    LOG_INFO("PlaybackPlugin", QString("Date changed"));
}

void PlaybackPlugin::onFileDoubleClicked(const RecordingFile& file)
{
    if (m_container) {
        QList<Clip> clips;
        Clip clip;
        clip.url = file.url;
        clip.startTimeMs = file.startTimeMs;
        clip.endTimeMs = file.endTimeMs;
        clip.durationMs = file.durationMs;
        clips.append(clip);

        QString targetCellId = m_container->getCellIds().value(0);
        if (targetCellId.isEmpty()) return;

        auto cell = m_container->getCell(targetCellId);
        if (cell) {
            cell->bindDevice(m_currentDeviceId);
            cell->loadClips(clips);
            cell->setController(m_container->controller());
            cell->setGroup(m_container->controller()->defaultGroup());
            m_container->setBaseTime(file.startTimeMs);
        }
    }
    LOG_INFO("PlaybackPlugin", QString("File double clicked: %1").arg(file.filename));
}

void PlaybackPlugin::loadRecordings(const QString& deviceId, const QDateTime& start, const QDateTime& end)
{
    if (m_container) {
        m_container->loadRecordings(deviceId, start.toMSecsSinceEpoch(), end.toMSecsSinceEpoch());
    }
}

}
