#include "PlaybackPlugin.h"
#include "core/EventBus.h"
#include "core/ServerApi.h"
#include "core/ReplayPlayerContainer.h"
#include "core/PlaybackSidePanel.h"
#include "core/PlaybackChannel.h"
#include "core/PlaybackController.h"
#include "core/PlaybackCell.h"
#include "core/PlaybackPlayerWidget.h"
#include "core/ReplayPlayer.h"
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
    workspace->setAttribute(Qt::WA_StyledBackground, true);

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
    // 通道的事件由侧栏转发（通道由侧栏拥有，连接在登记通道时建立）
    connect(m_sidePanel, &PlaybackSidePanel::channelFilesLoaded,
            this, &PlaybackPlugin::onChannelFilesLoaded);
    connect(m_sidePanel, &PlaybackSidePanel::channelRangeChanged,
            this, &PlaybackPlugin::onChannelRangeChanged);
    connect(m_sidePanel, &PlaybackSidePanel::channelAlarmsLoaded,
            this, &PlaybackPlugin::onChannelAlarmsLoaded);
    connect(m_sidePanel, &PlaybackSidePanel::channelBookmarksChanged,
            this, &PlaybackPlugin::onChannelBookmarksChanged);
    connect(m_sidePanel, &PlaybackSidePanel::channelPlayFileRequested,
            this, &PlaybackPlugin::onChannelPlayFileRequested);

    QWidget* rightPanel = new QWidget(workspace);
    rightPanel->setObjectName("playbackRightPanel");
    rightPanel->setAttribute(Qt::WA_StyledBackground, true);
    QVBoxLayout* rightLayout = new QVBoxLayout(rightPanel);
    rightLayout->setContentsMargins(4, 4, 4, 4);
    rightLayout->setSpacing(4);

    QToolBar* toolbar = new QToolBar(rightPanel);
    toolbar->setObjectName("playbackToolbar");

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
    toolbar->addWidget(speedLabel);

    QComboBox* speedCombo = new QComboBox(toolbar);
    speedCombo->addItems({"0.25x", "0.5x", "1x", "2x", "4x", "8x", "16x"});
    speedCombo->setCurrentText("1x");
    speedCombo->setProperty("class", "ToolbarComboBox");
    speedCombo->setFixedWidth(70);
    connect(speedCombo, &QComboBox::currentTextChanged, this, &PlaybackPlugin::onSpeedTextChanged);
    toolbar->addWidget(speedCombo);

    toolbar->addSeparator();

    QLabel* timeLabel = new QLabel(tr("Time:"), toolbar);
    toolbar->addWidget(timeLabel);

    m_timeEdit = new QDateTimeEdit(m_playbackTime, toolbar);
    m_timeEdit->setProperty("class", "ToolbarDateTimeEdit");
    m_timeEdit->setCalendarPopup(true);
    m_timeEdit->setFixedWidth(160);
    connect(m_timeEdit, &QDateTimeEdit::dateTimeChanged, this, &PlaybackPlugin::onTimeEditChanged);
    toolbar->addWidget(m_timeEdit);

    QLabel* currentTimeLabel = new QLabel("--:--:--", toolbar);
    currentTimeLabel->setObjectName("currentTimeLabel");
    toolbar->addWidget(currentTimeLabel);
    m_currentTimeLabel = currentTimeLabel;

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
    m_targetLabel->setObjectName("PlaybackTargetLabel");
    m_targetLabel->setToolTip(tr("点击录像文件会播放到这个窗口"));
    toolbar->addWidget(m_targetLabel);

    rightLayout->addWidget(toolbar);

    m_container = new ReplayPlayerContainer(rightPanel);
    connect(m_container, &ReplayPlayerContainer::cellClicked, this, &PlaybackPlugin::onCellClicked);
    // 播放窗口工具栏的关闭按钮：通道没有其它窗口在播放时从回放页注销
    connect(m_container, &ReplayPlayerContainer::channelCloseRequested,
            this, &PlaybackPlugin::onChannelClosed);
    // 播放进度同步到对应通道（书签位置用）
    connect(m_container, &ReplayPlayerContainer::channelPlaybackProgress,
            this, &PlaybackPlugin::onChannelPlaybackProgress);
    // 目标窗口变化：提示双击录像文件会播到哪里
    connect(m_container, &ReplayPlayerContainer::targetCellChanged,
            this, &PlaybackPlugin::onTargetCellChanged);
    // 录像文件拖到播放窗口：那个窗口播这个通道
    connect(m_container, &ReplayPlayerContainer::recordingDropped,
            this, &PlaybackPlugin::onRecordingDropped);
    // 设备树通道拖到播放窗口：绑定该通道，加载完录像后自动播放
    connect(m_container, &ReplayPlayerContainer::channelDropped,
            this, &PlaybackPlugin::onChannelDropped);
    rightLayout->addWidget(m_container, 1);

    m_container->initialize();

    mainLayout->addWidget(rightPanel, 1);

    QTimer* updateTimer = new QTimer(workspace);
    connect(updateTimer, &QTimer::timeout, this, &PlaybackPlugin::onUpdateTimer);
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
    // 工具栏只作用于当前激活通道
    const QString key = activeChannelKey();
    if (key.isEmpty() || !m_container) {
        return;
    }
    m_container->playChannel(key, 0);
    LOG_INFO("PlaybackPlugin", QString("Playback started: %1").arg(key));
}

void PlaybackPlugin::onPause()
{
    const QString key = activeChannelKey();
    if (key.isEmpty() || !m_container) {
        return;
    }
    m_container->pauseChannel(key);
    LOG_INFO("PlaybackPlugin", QString("Playback paused: %1").arg(key));
}

void PlaybackPlugin::onStop()
{
    const QString key = activeChannelKey();
    if (!key.isEmpty() && m_container) {
        m_container->stopChannel(key);
    }
    m_playbackTime = QDateTime();
    LOG_INFO("PlaybackPlugin", "Playback stopped");
}

void PlaybackPlugin::onSpeedTextChanged(const QString& text)
{
    bool ok = false;
    const double speed = text.left(text.size() - 1).toDouble(&ok);
    if (!ok) {
        return;
    }
    m_playbackSpeed = speed;
    const QString key = activeChannelKey();
    if (!key.isEmpty() && m_container) {
        m_container->setChannelSpeed(key, speed);
    }
    LOG_INFO("PlaybackPlugin", QString("Speed changed: %1x").arg(speed));
}

void PlaybackPlugin::onTimeEditChanged(const QDateTime& dt)
{
    m_playbackTime = dt;
    onSeek(dt.toMSecsSinceEpoch());
}

void PlaybackPlugin::onSeek(int64_t timeMs)
{
    if (!m_container || !m_sidePanel) {
        return;
    }
    // 时间输入框只作用到当前查看的通道
    const QString key = activeChannelKey();
    if (key.isEmpty()) {
        return;
    }
    m_container->seekChannel(key, timeMs);
    PlaybackChannel* channel = m_sidePanel->channel(key);
    if (channel) {
        channel->setPlaybackPosition(timeMs);
    }
}

void PlaybackPlugin::onUpdateTimer()
{
    // 显示当前查看通道的播放位置
    PlaybackChannel* channel = m_sidePanel ? m_sidePanel->activeChannel() : nullptr;
    if (!channel || !m_currentTimeLabel) {
        return;
    }
    const int64_t baseTime = channel->playbackPositionMs();
    if (baseTime > 0) {
        const QDateTime dt = QDateTime::fromMSecsSinceEpoch(baseTime);
        m_currentTimeLabel->setText(dt.toString("yyyy-MM-dd hh:mm:ss"));
    }
}

QString PlaybackPlugin::activeChannelKey() const {
    return m_sidePanel ? m_sidePanel->activeChannelKey() : QString();
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

void PlaybackPlugin::onCellClicked(const QString& layoutItemId, const QString& deviceId)
{
    Q_UNUSED(layoutItemId)
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
        // 通道由侧栏创建并连接，这里只补上播放侧的接线
        channel = m_sidePanel->registerChannel(deviceTag, channelTag, displayName);
        if (m_container) {
            m_container->attachChannelTimeline(key, channel->timeline());
            // 通道与播放器共用同一个播放组（通道的播放位置）
            channel->setGroup(m_container->channelGroup(key));
            // 时间轴上的定位直接调用容器 seek（目标唯一）
            channel->setContainer(m_container);
            // 时间轴挂上之后再刷一次书签，标记才能画到时间轴上
            channel->refreshBookmarks();
        }
    }
    return channel;
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
        applyChannelNameToCell(channel, targetCell);
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
        // 倍速跟随当前通道
        if (m_playbackSpeed != 1.0) {
            m_container->setChannelSpeed(key, m_playbackSpeed);
        }
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
    m_autoPlayChannels.remove(key);
    if (m_sidePanel) {
        m_sidePanel->unregisterChannel(key);
    }
}

void PlaybackPlugin::onChannelPlaybackProgress(const QString& channelKey, int64_t timeMs) {
    // 多个通道共用这一个槽：按 key 找到通道才更新
    PlaybackChannel* channel = m_sidePanel ? m_sidePanel->channel(channelKey) : nullptr;
    if (channel) {
        channel->setPlaybackPosition(timeMs);
    }
}

void PlaybackPlugin::onTargetCellChanged(const QString& layoutItemId) {
    m_targetCellId = layoutItemId;
    if (m_targetLabel) {
        m_targetLabel->setText(layoutItemId.isEmpty()
            ? tr("回放窗口: 未选择")
            : tr("回放窗口: %1").arg(layoutItemId));
    }
}

void PlaybackPlugin::onRecordingDropped(const QString& layoutItemId, const QString& channelKey,
                                        int64_t startTimeMs) {
    PlaybackChannel* channel = m_sidePanel ? m_sidePanel->channel(channelKey) : nullptr;
    if (!channel || !m_container) {
        return;
    }
    m_container->bindChannelToCell(channelKey, layoutItemId);
    applyChannelNameToCell(channel, layoutItemId);
    m_sidePanel->setActiveChannel(channelKey);
    playChannelFrom(channelKey, startTimeMs);
}

void PlaybackPlugin::onChannelDropped(const QString& layoutItemId, const QString& deviceTag,
                                      const QString& channelTag, const QString& displayName) {
    if (!m_container) {
        return;
    }
    PlaybackChannel* channel = ensureChannel(deviceTag, channelTag, displayName);
    if (!channel) {
        return;
    }
    m_container->selectCell(layoutItemId);
    m_autoPlayChannels.insert(channel->key());
    activateChannel(channel->key(), layoutItemId);
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
    if (m_container->isChannelPlaying(key)) {
        // 正在播放：只追加分片，不打断
        m_container->appendChannelClips(key, clips);
    } else {
        m_container->applyChannelClips(key, clips, range.first, range.second);
    }
    if (m_autoPlayChannels.remove(key)) {
        m_container->playChannel(key, 0);
    }
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
        applyChannelNameToCell(channel, cellId);
    }
    playChannelFrom(key, file.startTimeMs);
}

void PlaybackPlugin::applyChannelNameToCell(PlaybackChannel* channel, const QString& cellId) {
    if (!channel || !m_container || cellId.isEmpty()) {
        return;
    }
    PlaybackCell* cell = m_container->getCell(cellId);
    PlaybackPlayerWidget* widget = cell ? cell->playerWidget() : nullptr;
    ReplayPlayer* player = widget ? widget->player() : nullptr;
    if (!player) {
        return;
    }
    const QString name = channel->displayName().isEmpty() ? channel->channelTag() : channel->displayName();
    player->setDeviceName(name);
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
