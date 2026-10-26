#include "PlaybackPlugin.h"
#include "core/EventBus.h"
#include "core/ServerApi.h"
#include "core/ReplayPlayerContainer.h"
#include "core/PlaybackSidePanel.h"
#include "core/PlaybackChannel.h"
#include "core/PlaybackController.h"
#include "core/PlaybackCell.h"
#include "core/ClipExportDialog.h"
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
#include <QMessageBox>
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
    m_currentDeviceTag.clear();
    m_currentChannelTag.clear();
    m_targetCellId.clear();
    m_currentPositionMs = 0;
    m_targetLabel = nullptr;
    m_timeEdit = nullptr;

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
    m_workspace = workspace;
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

    // 侧栏（设备/通道/文件列表）与播放器之间的连线
    connect(m_sidePanel, &PlaybackSidePanel::channelRequested,
            this, &PlaybackPlugin::onChannelRequested);
    connect(m_sidePanel, &PlaybackSidePanel::activeChannelChanged,
            this, &PlaybackPlugin::onActiveChannelChanged);
    connect(m_sidePanel, &PlaybackSidePanel::channelClosed,
            this, &PlaybackPlugin::onChannelClosed);

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

    m_timeEdit = new QDateTimeEdit(m_playbackTime, toolbar);
    m_timeEdit->setProperty("class", "ToolbarDateTimeEdit");
    m_timeEdit->setCalendarPopup(true);
    m_timeEdit->setFixedWidth(160);
    connect(m_timeEdit, &QDateTimeEdit::dateTimeChanged, this, [this](const QDateTime& dt) {
        m_playbackTime = dt;
        onSeek(dt.toMSecsSinceEpoch());
    });
    toolbar->addWidget(m_timeEdit);

    QLabel* currentTimeLabel = new QLabel("--:--:--", toolbar);
    currentTimeLabel->setObjectName("currentTimeLabel");
    currentTimeLabel->setStyleSheet("color: #e0e0e0; font-family: monospace; min-width: 150px;");
    toolbar->addWidget(currentTimeLabel);

    QPushButton* exportBtn = new QPushButton(tr("导出片段"), toolbar);
    exportBtn->setCursor(Qt::PointingHandCursor);
    exportBtn->setProperty("class", "ToolbarButton");
    exportBtn->setFixedHeight(32);
    exportBtn->setToolTip(tr("把当前选中的录像按时间区间裁剪导出"));
    connect(exportBtn, &QPushButton::clicked, this, &PlaybackPlugin::onExportClip);
    toolbar->addWidget(exportBtn);

    QWidget* spacer = new QWidget(toolbar);
    spacer->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Preferred);
    toolbar->addWidget(spacer);

    m_targetLabel = new QLabel(tr("回放窗口: 未选择"), toolbar);
    m_targetLabel->setStyleSheet("color: #9a9a9a;");
    m_targetLabel->setToolTip(tr("点击录像文件会播放到这个窗口"));
    toolbar->addWidget(m_targetLabel);

    rightLayout->addWidget(toolbar);

    m_container = new ReplayPlayerContainer(rightPanel);
    connect(m_container, &ReplayPlayerContainer::cellClicked, this, &PlaybackPlugin::onCellClicked);
    // 播放进度同步到对应通道（书签位置用）
    connect(m_container, &ReplayPlayerContainer::playbackProgress, this, [this](int64_t timeMs) {
        m_currentPositionMs = timeMs;
    });
    connect(m_container, &ReplayPlayerContainer::channelPlaybackProgress, this,
            [this](const QString& channelKey, int64_t timeMs) {
                PlaybackChannel* channel = m_sidePanel ? m_sidePanel->channel(channelKey) : nullptr;
                if (channel) {
                    channel->setPlaybackPosition(timeMs);
                }
            });
    // 目标窗口变化：提示双击录像文件会播到哪里
    connect(m_container, &ReplayPlayerContainer::targetCellChanged, this, [this](const QString& cellId) {
        m_targetCellId = cellId;
        if (m_targetLabel) {
            m_targetLabel->setText(cellId.isEmpty()
                ? tr("回放窗口: 未选择")
                : tr("回放窗口: %1").arg(cellId));
        }
    });
    // 录像文件拖到播放窗口：那个窗口播这个通道
    connect(m_container, &ReplayPlayerContainer::recordingDropped, this,
            [this](const QString& channelKey, int64_t startTimeMs, const QString& cellId) {
                PlaybackChannel* channel = m_sidePanel ? m_sidePanel->channel(channelKey) : nullptr;
                if (!channel || !m_container) {
                    return;
                }
                m_container->bindChannelToCell(channelKey, cellId);
                m_sidePanel->setActiveChannel(channelKey);
                playChannelFrom(channelKey, startTimeMs);
            });
    rightLayout->addWidget(m_container, 1);

    m_container->initialize();

    mainLayout->addWidget(rightPanel, 1);

    QTimer* updateTimer = new QTimer(workspace);
    connect(updateTimer, &QTimer::timeout, this, [this, currentTimeLabel]() {
        // 显示当前查看通道的播放位置
        PlaybackChannel* channel = m_sidePanel ? m_sidePanel->activeChannel() : nullptr;
        if (!channel) {
            return;
        }
        const int64_t baseTime = channel->playbackPositionMs();
        if (baseTime > 0) {
            QDateTime dt = QDateTime::fromMSecsSinceEpoch(baseTime);
            currentTimeLabel->setText(dt.toString("yyyy-MM-dd hh:mm:ss"));
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

    // deviceId 形如 "设备tag/通道tag"，服务端需要分开的两个参数
    const int slash = deviceId.indexOf('/');
    const QString deviceTag = slash > 0 ? deviceId.left(slash) : deviceId;
    const QString channelTag = slash > 0 ? deviceId.mid(slash + 1) : QString();

    onDeviceSelected(deviceTag, channelTag, deviceId);

    // 跳到请求的日期和时间点
    PlaybackChannel* channel = ensureChannel(deviceTag, channelTag, deviceId);
    if (channel) {
        channel->onDateChanged(startTime.date());
        activateChannel(channel->key(), QString());
        onSeek(startTime.toMSecsSinceEpoch());
    }

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
    if (!m_container || !m_sidePanel) {
        return;
    }
    // 时间输入框只作用到当前查看的通道
    const QString key = m_sidePanel->activeChannelKey();
    if (key.isEmpty()) {
        return;
    }
    m_container->seekChannel(key, timeMs);
    PlaybackChannel* channel = m_sidePanel->channel(key);
    if (channel) {
        channel->setPlaybackPosition(timeMs);
    }
}

void PlaybackPlugin::updateTimeEdit(int64_t timeMs) {
    if (!m_timeEdit || timeMs <= 0) {
        return;
    }
    const QDateTime dt = QDateTime::fromMSecsSinceEpoch(timeMs);
    if (m_timeEdit->dateTime() != dt) {
        m_timeEdit->setDateTime(dt);
    }
}

void PlaybackPlugin::onCellClicked(int cellId, const QString& deviceId)
{
    Q_UNUSED(cellId)
    Q_UNUSED(deviceId)
}

void PlaybackPlugin::onDeviceSelected(const QString& deviceTag, const QString& channelTag, const QString& deviceName) {
    Q_UNUSED(deviceName)
    if (deviceTag.isEmpty() && channelTag.isEmpty()) {
        LOG_WARNING("PlaybackPlugin", "Empty device selected");
        return;
    }

    m_currentDeviceTag = deviceTag;
    m_currentChannelTag = channelTag;
    m_currentDeviceId = channelTag.isEmpty() ? deviceTag : deviceTag + "/" + channelTag;

    LOG_INFO("PlaybackPlugin", QString("Device selected: device=%1 channel=%2").arg(deviceTag).arg(channelTag));
}

// ── 通道 ────────────────────────────────────────────────────────────

PlaybackChannel* PlaybackPlugin::ensureChannel(const QString& deviceTag, const QString& channelTag,
                                                const QString& displayName) {
    if (!m_sidePanel) {
        return nullptr;
    }
    const QString key = deviceTag + "/" + channelTag;
    PlaybackChannel* channel = m_sidePanel->channel(key);
    if (!channel) {
        channel = m_sidePanel->registerChannel(deviceTag, channelTag, displayName);
        attachChannelSignals(key, channel);
        if (m_container) {
            m_container->attachChannelTimeline(key, channel->timeline());
            // 通道与播放器共用同一个播放组（通道的播放位置）
            channel->setGroup(m_container->channelGroup(key));
        }
    }
    return channel;
}

void PlaybackPlugin::attachChannelSignals(const QString& key, PlaybackChannel* channel) {
    connect(channel, &PlaybackChannel::filesLoaded, this, [this, key](const QList<RecordingFile>& files) {
        onChannelFilesLoaded(key, files);
    });
    connect(channel, &PlaybackChannel::rangeChanged, this, [this, key](int64_t startMs, int64_t endMs) {
        onChannelRangeChanged(key, startMs, endMs);
    });
    connect(channel, &PlaybackChannel::alarmsLoaded, this, [this, key](const QList<TimelineEvent>& events) {
        onChannelAlarmsLoaded(key, events);
    });
    connect(channel, &PlaybackChannel::bookmarksChanged, this, [this, key](const QList<TimelineEvent>& bookmarks) {
        onChannelBookmarksChanged(key, bookmarks);
    });
    connect(channel, &PlaybackChannel::seekRequested, this, [this, key](int64_t timeMs) {
        onChannelSeekRequested(key, timeMs);
    });
    connect(channel, &PlaybackChannel::playFileRequested, this, [this, key](const RecordingFile& file) {
        onChannelPlayFileRequested(key, file);
    });
}

void PlaybackPlugin::activateChannel(const QString& key, const QString& cellId) {
    if (!m_sidePanel || !m_container) {
        return;
    }
    PlaybackChannel* channel = m_sidePanel->channel(key);
    if (!channel) {
        return;
    }
    QString targetCell = cellId;
    if (targetCell.isEmpty()) {
        targetCell = m_container->selectedCellId();
    }
    if (!targetCell.isEmpty()) {
        m_container->bindChannelToCell(key, targetCell);
    }
    m_sidePanel->setActiveChannel(key);
    channel->reload();
}

void PlaybackPlugin::onChannelRequested(const QString& deviceTag, const QString& channelTag,
                                        const QString& displayName) {
    PlaybackChannel* channel = ensureChannel(deviceTag, channelTag, displayName);
    if (!channel) {
        return;
    }
    activateChannel(channel->key(), QString());
}

void PlaybackPlugin::onActiveChannelChanged(const QString& key) {
    if (m_container) {
        m_container->setActiveChannelTimeline(key);
    }
    PlaybackChannel* channel = m_sidePanel ? m_sidePanel->channel(key) : nullptr;
    if (channel) {
        updateTimeEdit(channel->playbackPositionMs());
    }
}

void PlaybackPlugin::onChannelClosed(const QString& key) {
    if (m_container) {
        for (const QString& cellId : m_container->cellsOfChannel(key)) {
            m_container->unbindCell(cellId);
        }
        m_container->removeChannelTimeline(key);
    }
    m_channelRanges.remove(key);
    if (m_sidePanel) {
        m_sidePanel->unregisterChannel(key);
    }
}

void PlaybackPlugin::onChannelFilesLoaded(const QString& key, const QList<RecordingFile>& files) {
    QList<Clip> clips;
    QList<RecordingPeriod> periods;
    clips.reserve(files.size());
    periods.reserve(files.size());
    for (const RecordingFile& file : files) {
        if (file.url.isEmpty() || !file.timeValid) {
            continue;
        }
        Clip clip;
        clip.url = file.url;
        clip.startTimeMs = file.startTimeMs;
        clip.endTimeMs = file.endTimeMs;
        clip.durationMs = file.durationMs;
        clip.segmentType = file.segmentType;
        clip.alarmId = file.alarmId;
        clips.append(clip);

        RecordingPeriod period;
        period.startTimeMs = file.startTimeMs;
        period.endTimeMs = file.endTimeMs;
        period.url = file.url;
        period.fileId = file.id;
        period.segmentType = file.segmentType;
        period.alarmId = file.alarmId;
        periods.append(period);
    }

    if (!m_container) {
        return;
    }
    const QPair<int64_t, int64_t> range = m_channelRanges.value(key, qMakePair<int64_t, int64_t>(0, 0));
    m_container->applyChannelClips(key, clips, range.first, range.second);
    TimelineWidget* timeline = m_container->channelTimeline(key);
    if (timeline) {
        timeline->setPeriods(periods);
    }
}

void PlaybackPlugin::onChannelRangeChanged(const QString& key, int64_t startMs, int64_t endMs) {
    m_channelRanges.insert(key, qMakePair(startMs, endMs));
}

void PlaybackPlugin::onChannelAlarmsLoaded(const QString& key, const QList<TimelineEvent>& events) {
    TimelineWidget* timeline = m_container ? m_container->channelTimeline(key) : nullptr;
    if (timeline) {
        timeline->setEvents(events);
    }
}

void PlaybackPlugin::onChannelBookmarksChanged(const QString& key, const QList<TimelineEvent>& bookmarks) {
    TimelineWidget* timeline = m_container ? m_container->channelTimeline(key) : nullptr;
    if (timeline) {
        timeline->setBookmarks(bookmarks);
    }
}

void PlaybackPlugin::onChannelSeekRequested(const QString& key, int64_t timeMs) {
    if (!m_container) {
        return;
    }
    m_container->seekChannel(key, timeMs);
    PlaybackChannel* channel = m_sidePanel ? m_sidePanel->channel(key) : nullptr;
    if (channel) {
        channel->setPlaybackPosition(timeMs);
    }
}

void PlaybackPlugin::onChannelPlayFileRequested(const QString& key, const RecordingFile& file) {
    if (!m_sidePanel || !m_container) {
        return;
    }
    PlaybackChannel* channel = m_sidePanel->channel(key);
    if (!channel) {
        return;
    }
    // 双击文件：播放到当前选中的窗口（该窗口因此绑定这个通道）
    const QString cellId = m_container->selectedCellId();
    if (!cellId.isEmpty()) {
        m_container->bindChannelToCell(key, cellId);
    }
    playChannelFrom(key, file.startTimeMs);
}

void PlaybackPlugin::playChannelFrom(const QString& key, int64_t startTimeMs) {
    if (!m_sidePanel || !m_container) {
        return;
    }
    PlaybackChannel* channel = m_sidePanel->channel(key);
    if (!channel) {
        return;
    }
    QList<Clip> clips;
    clips.reserve(channel->files().size());
    for (const RecordingFile& f : channel->files()) {
        if (f.url.isEmpty() || !f.timeValid) {
            continue;
        }
        Clip clip;
        clip.url = f.url;
        clip.startTimeMs = f.startTimeMs;
        clip.endTimeMs = f.endTimeMs;
        clip.durationMs = f.durationMs;
        clip.segmentType = f.segmentType;
        clip.alarmId = f.alarmId;
        clips.append(clip);
    }
    if (clips.isEmpty()) {
        return;
    }
    const QPair<int64_t, int64_t> range = m_channelRanges.value(key, qMakePair<int64_t, int64_t>(0, 0));
    m_container->applyChannelClips(key, clips, range.first, range.second);
    m_container->playChannel(key, startTimeMs);
    channel->setPlaybackPosition(startTimeMs);
    updateTimeEdit(startTimeMs);
}

void PlaybackPlugin::onFileDoubleClicked(const RecordingFile& file) {
    Q_UNUSED(file)
}

void PlaybackPlugin::onExportClip() {
    if (!m_sidePanel) {
        return;
    }
    PlaybackChannel* channel = m_sidePanel->activeChannel();
    if (!channel) {
        QMessageBox::warning(m_workspace, tr("无法导出"), tr("请先选择一个通道。"));
        return;
    }
    const RecordingFile file = channel->fileList()->currentFile();
    if (file.url.isEmpty()) {
        QMessageBox::warning(m_workspace, tr("无法导出"), tr("请先在录像文件列表中选择一个文件。"));
        return;
    }
    ClipExportDialog dialog(file, m_workspace);
    dialog.exec();
}

}
