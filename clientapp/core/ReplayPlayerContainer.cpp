#include "ReplayPlayerContainer.h"
#include "PlaybackCell.h"
#include "PlaybackController.h"
#include "PlaybackPlayerWidget.h"
#include "ReplayPlayer.h"
#include "ReplayNavBar.h"
#include "ServerApi.h"
#include "common/Logger.h"
#include <QVBoxLayout>
#include <QStackedWidget>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonDocument>
#include <QDateTime>
#include <QDragEnterEvent>
#include <QDragMoveEvent>
#include <QDropEvent>
#include <QEvent>
#include <QtConcurrent>
#include <QFutureWatcher>
#include <QMessageBox>

namespace rustcam {

namespace {
const char* const kDeviceMimeType = "application/x-rustcam-device";
}

ReplayPlayerContainer::ReplayPlayerContainer(QWidget* parent)
    : QWidget(parent)
    , m_videoArea(nullptr)
    , m_navBar(nullptr)
    , m_timelineStack(nullptr)
    , m_controller(nullptr)
    , m_totalRecords(0)
    , m_page(1)
    , m_limit(50)
    , m_hasMore(false)
{
    QVBoxLayout* layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    m_videoArea = new QWidget(this);
    m_videoArea->setObjectName("videoArea");
    // 时间轴出现/切换会改变视频区的实际高度，单元格必须据此重新排布，
    // 否则底部一行会伸到时间轴下面（被时间轴遮住）。
    m_videoArea->installEventFilter(this);

    // 每个通道一条时间轴，按当前通道切换显示
    m_timelineStack = new QStackedWidget(this);
    m_timelineStack->setObjectName("timelineStack");

    m_navBar = new ReplayNavBar(this);
    m_navBar->setObjectName("navBar");

    m_controller = new PlaybackController(this);

    layout->addWidget(m_videoArea, 1);
    layout->addWidget(m_timelineStack, 0);
    layout->addWidget(m_navBar, 0);

    connect(m_navBar, &ReplayNavBar::layoutSelected, this, &ReplayPlayerContainer::onLayoutSelected);
}

ReplayPlayerContainer::~ReplayPlayerContainer() {
    for (PlaybackCell* cell : m_cells.values()) {
        delete cell;
    }
    m_cells.clear();
}

void ReplayPlayerContainer::initialize() {
    if (DataManager::instance()->isLayoutsLoaded()) {
        onLayoutsReady();
    } else {
        connect(DataManager::instance(), &DataManager::layoutsLoaded, this, &ReplayPlayerContainer::onLayoutsReady, Qt::UniqueConnection);
        DataManager::instance()->loadLayouts();
    }
}

void ReplayPlayerContainer::onLayoutsReady() {
    disconnect(DataManager::instance(), &DataManager::layoutsLoaded, this, &ReplayPlayerContainer::onLayoutsReady);
    m_layouts = DataManager::instance()->layouts();
    LOG_INFO("ReplayPlayerContainer", QString("Using layouts from DataManager: %1").arg(m_layouts.size()));

    m_navBar->setLayouts(m_layouts);

    if (!m_layouts.isEmpty()) {
        m_currentLayout = m_layouts.first();
        calculateLayout(m_currentLayout);
    }
}

void ReplayPlayerContainer::onLayoutSelected(int index) {
    if (index < 0 || index >= m_layouts.size()) return;

    m_selectedCellId.clear();
    m_currentLayout = m_layouts[index];
    calculateLayout(m_currentLayout);
}

void ReplayPlayerContainer::calculateLayout(const PlayerLayout& layout) {
    const int margin = 2;
    const int spacing = 2;

    int availableW = m_videoArea->width();
    int availableH = m_videoArea->height();

    if (availableW <= 0 || availableH <= 0 || layout.cols <= 0 || layout.rows <= 0) {
        return;
    }

    int cw = (availableW - margin * 2 - spacing * (layout.cols - 1)) / layout.cols;
    int ch = (availableH - margin * 2 - spacing * (layout.rows - 1)) / layout.rows;

    QList<PlaybackCell*> oldCells;
    for (const QString& itemId : m_cellOrder) {
        if (PlaybackCell* cell = m_cells.value(itemId, nullptr)) {
            oldCells.append(cell);
        }
    }
    QList<PlaybackCell*> cellsToKeep;
    int cellIndex = 0;
    int reusedCount = 0;
    int newCount = 0;

    // 布局切换：窗口按 index 复用。通道绑定跟着播放器走（cell 自己记住播哪个通道），
    // 窗口换位置/被删除都不影响绑定关系。
    for (const LayoutItem& item : layout.layoutJson) {
        int x = margin + item.col * (cw + spacing);
        int y = margin + item.row * (ch + spacing);
        int w = item.colSpan * cw + (item.colSpan - 1) * spacing;
        int h = item.rowSpan * ch + (item.rowSpan - 1) * spacing;

        PlaybackCell* cell = nullptr;
        if (cellIndex < oldCells.size()) {
            cell = oldCells[cellIndex];
            reusedCount++;
        } else {
            cell = new PlaybackCell(m_videoArea);
            // 格子由容器拥有：连接在这里一次建立，之后不再变动
            connect(cell, &PlaybackCell::clicked, this, &ReplayPlayerContainer::onCellClicked);
            connect(cell, &PlaybackCell::playbackProgressChanged,
                    this, &ReplayPlayerContainer::onCellProgressChanged);
            connect(cell, &PlaybackCell::playerDropped, this, &ReplayPlayerContainer::onPlayerDropped);
            connect(cell, &PlaybackCell::recordingDropped,
                    this, &ReplayPlayerContainer::onRecordingDropped);
            connect(cell, &PlaybackCell::channelDropped, this, &ReplayPlayerContainer::onChannelDropped);
            connect(cell, &PlaybackCell::closeRequested, this, &ReplayPlayerContainer::closeCell);
            newCount++;
        }

        // 格子在布局中的位置标识（发射信号时由格子自己带上，不会过期）
        cell->setLayoutItemId(item.id);
        cell->setGeometry(x, y, w, h);
        cell->show();
        cellsToKeep.append(cell);
        cellIndex++;
    }

    // 被删掉的格子：清掉播放器的通道绑定与时间轴播放头
    for (int i = cellsToKeep.size(); i < oldCells.size(); ++i) {
        PlaybackCell* dead = oldCells[i];
        PlaybackPlayerWidget* player = dead->playerWidget();
        if (player) {
            const QString channelKey = player->channelKey();
            if (!channelKey.isEmpty()) {
                TimelineWidget* timeline = channelTimeline(channelKey);
                if (timeline) {
                    timeline->removePlayhead(player->uid());
                }
                m_playheads.remove(player->uid());
            }
            delete player;
        }
        delete dead;
    }

    m_cells.clear();
    m_cellOrder.clear();
    for (int i = 0; i < layout.layoutJson.size(); ++i) {
        m_cells.insert(layout.layoutJson[i].id, cellsToKeep[i]);
        m_cellOrder.append(layout.layoutJson[i].id);
    }

    applySelectionStates();

    LOG_INFO("ReplayPlayerContainer", QString("calculateLayout: reused %1, created %2, deleted %3")
        .arg(reusedCount).arg(newCount).arg(oldCells.size() - reusedCount));
}

void ReplayPlayerContainer::onPlayerDropped(const QString& targetLayoutItemId, const QString& sourceUid) {
    PlaybackCell* target = m_cells.value(targetLayoutItemId, nullptr);
    PlaybackCell* source = findCellByUid(sourceUid);
    if (!source || !target || source == target) {
        return;
    }

    // 交换两个格子的位置与布局标识。
    // 播放器留在原来的父窗口里（父窗口不变 → 窗口句柄不变 → 画面不用重建），
    // 通道绑定跟着播放器走。
    const QRect sourceRect = source->geometry();
    const QRect targetRect = target->geometry();
    const QString sourceItemId = source->layoutItemId();
    const QString targetItemId = target->layoutItemId();

    source->setGeometry(targetRect);
    target->setGeometry(sourceRect);
    source->setLayoutItemId(targetItemId);
    target->setLayoutItemId(sourceItemId);
    m_cells.insert(targetItemId, source);
    m_cells.insert(sourceItemId, target);

    applySelectionStates();

    LOG_INFO("ReplayPlayerContainer", QString("swap players: %1 <-> %2")
        .arg(sourceItemId, targetItemId));
}

void ReplayPlayerContainer::applySelectionStates() {
    for (auto it = m_cells.begin(); it != m_cells.end(); ++it) {
        it.value()->setSelected(it.key() == m_selectedCellId);
    }
}

PlaybackCell* ReplayPlayerContainer::findCellByUid(const QString& uid) const {
    for (auto it = m_cells.constBegin(); it != m_cells.constEnd(); ++it) {
        PlaybackPlayerWidget* player = it.value()->playerWidget();
        if (player && player->uid() == uid) {
            return it.value();
        }
    }
    return nullptr;
}

PlaybackCell* ReplayPlayerContainer::getCell(const QString& cellId) const {
    return m_cells.value(cellId, nullptr);
}

QString ReplayPlayerContainer::selectTargetCell() const {
    if (!m_selectedCellId.isEmpty() && m_cells.contains(m_selectedCellId)) {
        return m_selectedCellId;
    }

    if (!m_cells.isEmpty()) {
        return m_cells.firstKey();
    }

    return QString();
}

void ReplayPlayerContainer::onCellClicked(const QString& layoutItemId, const QString& deviceId) {
    // 多个格子共用这一个槽：按信号带来的布局项 id 定位目标
    PlaybackCell* cell = m_cells.value(layoutItemId, nullptr);
    if (!cell) {
        return;
    }

    const QString oldSelected = m_selectedCellId;
    m_selectedCellId = layoutItemId;

    if (!oldSelected.isEmpty() && oldSelected != layoutItemId) {
        PlaybackCell* oldCell = m_cells.value(oldSelected, nullptr);
        if (oldCell) {
            oldCell->setSelected(false);
        }
    }

    cell->setSelected(true);
    emit cellClicked(layoutItemId, deviceId);

    if (m_selectedCellId != oldSelected) {
        emit targetCellChanged(m_selectedCellId);
    }
}

void ReplayPlayerContainer::onCellProgressChanged(const QString& layoutItemId, int64_t timeMs)
{
    // 多个播放器同时上报时这个槽会被调多次：按布局项 id 定位播放头所属的通道
    PlaybackCell* cell = m_cells.value(layoutItemId, nullptr);
    PlaybackPlayerWidget* player = cell ? cell->playerWidget() : nullptr;
    if (!player) {
        return;
    }
    const QString channelKey = player->channelKey();
    if (channelKey.isEmpty()) {
        return;
    }
    emit channelPlaybackProgress(channelKey, timeMs);
    // 播放头画在播放器所属通道的时间轴上
    TimelineWidget* timeline = channelTimeline(channelKey);
    if (timeline) {
        timeline->updatePlayhead(player->uid(), timeMs);
    }
}

void ReplayPlayerContainer::onRecordingDropped(const QString& layoutItemId, const QString& channelKey,
                                               int64_t startTimeMs) {
    emit recordingDropped(layoutItemId, channelKey, startTimeMs);
}

void ReplayPlayerContainer::onChannelDropped(const QString& layoutItemId, const QString& deviceTag,
                                             const QString& channelTag, const QString& displayName) {
    emit channelDropped(layoutItemId, deviceTag, channelTag, displayName);
}

void ReplayPlayerContainer::setBaseTime(int64_t timeMs) {
    LOG_DEBUG("ReplayPlayerContainer", QString("setBaseTime: timeMs=%1").arg(timeMs));
    if (m_controller) {
        m_controller->setBaseTime(timeMs);
    }
    TimelineWidget* timeline = activeTimeline();
    if (timeline) {
        timeline->setCurrentTime(timeMs);
    }
}

// ── 通道时间轴 ──────────────────────────────────────────────────────

void ReplayPlayerContainer::attachChannelTimeline(const QString& channelKey, TimelineWidget* timeline) {
    if (!timeline || m_channelTimelines.contains(channelKey)) {
        return;
    }
    m_channelTimelines.insert(channelKey, timeline);
    m_timelineStack->addWidget(timeline);

    // 时间轴上的定位由通道自己处理（PlaybackChannel -> 播放器容器），这里只负责显示

    if (m_activeChannelKey.isEmpty()) {
        setActiveChannelTimeline(channelKey);
    }
}

void ReplayPlayerContainer::removeChannelTimeline(const QString& channelKey) {
    TimelineWidget* timeline = m_channelTimelines.take(channelKey);
    if (!timeline) {
        return;
    }
    if (m_activeChannelKey == channelKey) {
        m_activeChannelKey.clear();
    }
    m_timelineStack->removeWidget(timeline);
    timeline->deleteLater();
}

void ReplayPlayerContainer::setActiveChannelTimeline(const QString& channelKey) {
    TimelineWidget* timeline = m_channelTimelines.value(channelKey, nullptr);
    if (!timeline) {
        return;
    }
    m_activeChannelKey = channelKey;
    m_timelineStack->setCurrentWidget(timeline);
}

TimelineWidget* ReplayPlayerContainer::channelTimeline(const QString& channelKey) const {
    return m_channelTimelines.value(channelKey, nullptr);
}

TimelineWidget* ReplayPlayerContainer::activeTimeline() const {
    return m_channelTimelines.value(m_activeChannelKey, nullptr);
}

// ── 窗口 ↔ 通道 ─────────────────────────────────────────────────────

QString ReplayPlayerContainer::selectedCellId() const {
    if (!m_selectedCellId.isEmpty() && m_cells.contains(m_selectedCellId)) {
        return m_selectedCellId;
    }
    return QString();
}

void ReplayPlayerContainer::selectCell(const QString& cellId) {
    if (!m_cells.contains(cellId) || m_selectedCellId == cellId) {
        return;
    }
    m_selectedCellId = cellId;
    applySelectionStates();
    emit targetCellChanged(cellId);
}

PlaybackGroup* ReplayPlayerContainer::channelGroup(const QString& channelKey) {
    PlaybackGroup* group = m_channelGroups.value(channelKey, nullptr);
    if (!group) {
        group = m_controller->createGroup();
        m_channelGroups.insert(channelKey, group);
    }
    return group;
}

bool ReplayPlayerContainer::bindChannelToCell(const QString& channelKey, const QString& cellId) {
    PlaybackCell* cell = m_cells.value(cellId, nullptr);
    PlaybackPlayerWidget* player = cell ? cell->playerWidget() : nullptr;
    if (!player) {
        return false;
    }
    const QString oldKey = player->channelKey();
    if (!oldKey.isEmpty() && oldKey != channelKey) {
        // 原来播的是别的通道：先解绑
        unbindCell(cellId);
    }
    // 绑定记在播放器自己身上：播放器在格子之间切换也不用重新绑定
    player->setChannelKey(channelKey);
    LOG_INFO("ReplayPlayerContainer", QString("bindChannelToCell: cell=%1 channel=%2").arg(cellId, channelKey));
    emit cellChannelChanged(cellId, channelKey);
    return true;
}

void ReplayPlayerContainer::unbindCell(const QString& cellId) {
    PlaybackCell* cell = m_cells.value(cellId, nullptr);
    PlaybackPlayerWidget* player = cell ? cell->playerWidget() : nullptr;
    if (!player) {
        return;
    }
    const QString channelKey = player->channelKey();
    player->setChannelKey(QString());
    player->loadClips(QList<Clip>());
    if (channelKey.isEmpty()) {
        return;
    }
    TimelineWidget* timeline = channelTimeline(channelKey);
    if (timeline) {
        timeline->removePlayhead(player->uid());
    }
    m_playheads.remove(player->uid());
}

void ReplayPlayerContainer::closeCell(const QString& cellId) {
    PlaybackCell* cell = m_cells.value(cellId, nullptr);
    PlaybackPlayerWidget* player = cell ? cell->playerWidget() : nullptr;
    if (!player) {
        return;
    }

    const QString channelKey = player->channelKey();
    if (ReplayPlayer* replay = player->player()) {
        replay->stop();
    }
    if (channelKey.isEmpty()) {
        return;
    }

    // 同一通道可能绑定了多个窗口：只要还有其它窗口在播放，就只清空本格；
    // 否则说明该通道的录像已经没有任何窗口在播放，走完整的通道注销流程。
    bool otherPlaying = false;
    for (const QString& otherId : cellsOfChannel(channelKey)) {
        if (otherId == cellId) {
            continue;
        }
        PlaybackCell* otherCell = m_cells.value(otherId, nullptr);
        PlaybackPlayerWidget* other = otherCell ? otherCell->playerWidget() : nullptr;
        if (other && other->player() && other->player()->isPlaying()) {
            otherPlaying = true;
            break;
        }
    }

    if (otherPlaying) {
        unbindCell(cellId);
    } else {
        emit channelCloseRequested(channelKey);
    }
}

QString ReplayPlayerContainer::channelOfCell(const QString& cellId) const {
    PlaybackCell* cell = m_cells.value(cellId, nullptr);
    PlaybackPlayerWidget* player = cell ? cell->playerWidget() : nullptr;
    return player ? player->channelKey() : QString();
}

QStringList ReplayPlayerContainer::cellsOfChannel(const QString& channelKey) const {
    QStringList ids;
    for (auto it = m_cells.constBegin(); it != m_cells.constEnd(); ++it) {
        PlaybackPlayerWidget* player = it.value()->playerWidget();
        if (player && player->channelKey() == channelKey) {
            ids.append(it.key());
        }
    }
    return ids;
}

void ReplayPlayerContainer::applyChannelClips(const QString& channelKey, const QList<Clip>& clips,
                                              int64_t rangeStartMs, int64_t rangeEndMs) {
    PlaybackGroup* group = channelGroup(channelKey);
    const QStringList cellIds = cellsOfChannel(channelKey);
    if (cellIds.isEmpty()) {
        return;
    }
    TimelineWidget* timeline = channelTimeline(channelKey);
    for (const QString& cellId : cellIds) {
        PlaybackCell* cell = m_cells.value(cellId, nullptr);
        PlaybackPlayerWidget* player = cell ? cell->playerWidget() : nullptr;
        if (!player) {
            continue;
        }
        player->bindDevice(channelKey);
        player->loadClips(clips);
        player->setController(m_controller);
        player->setGroup(group);
        if (timeline && !timeline->playheads().contains(player->uid())) {
            timeline->addPlayhead(player->uid(), QColor("#4a9eff"));
            m_playheads.insert(player->uid());
        }
    }

    // 播放位置不在范围内时回到第一段
    int64_t base = m_controller->groupBaseTime(group);
    if (base <= 0 || (rangeEndMs > rangeStartMs && (base < rangeStartMs || base > rangeEndMs))) {
        if (!clips.isEmpty()) {
            m_controller->setGroupBaseTime(group, clips.first().startTimeMs);
        }
    }
}

void ReplayPlayerContainer::appendChannelClips(const QString& channelKey, const QList<Clip>& clips) {
    if (clips.isEmpty()) {
        return;
    }
    for (const QString& cellId : cellsOfChannel(channelKey)) {
        PlaybackCell* cell = m_cells.value(cellId, nullptr);
        PlaybackPlayerWidget* player = cell ? cell->playerWidget() : nullptr;
        if (!player || !player->source()) {
            continue;
        }
        // 追加而不是替换：正在进行的播放不中断
        player->source()->appendClips(clips);
    }
}

bool ReplayPlayerContainer::isChannelPlaying(const QString& channelKey) const {
    for (const QString& cellId : cellsOfChannel(channelKey)) {
        PlaybackCell* cell = m_cells.value(cellId, nullptr);
        PlaybackPlayerWidget* player = cell ? cell->playerWidget() : nullptr;
        if (player && player->source()
            && player->source()->state() == StreamSource::State::Playing) {
            return true;
        }
    }
    return false;
}

void ReplayPlayerContainer::seekChannel(const QString& channelKey, int64_t timeMs) {
    PlaybackGroup* group = m_channelGroups.value(channelKey, nullptr);
    if (!group) {
        return;
    }
    m_controller->setGroupBaseTime(group, timeMs);
    TimelineWidget* timeline = channelTimeline(channelKey);
    if (timeline) {
        timeline->setCurrentTime(timeMs);
    }
}

void ReplayPlayerContainer::playChannel(const QString& channelKey, int64_t timeMs) {
    PlaybackGroup* group = m_channelGroups.value(channelKey, nullptr);
    if (!group) {
        return;
    }
    if (timeMs > 0) {
        m_controller->setGroupBaseTime(group, timeMs);
    }
    group->play();
}

void ReplayPlayerContainer::pauseChannel(const QString& channelKey) {
    PlaybackGroup* group = m_channelGroups.value(channelKey, nullptr);
    if (!group) {
        return;
    }
    group->pause();
}

void ReplayPlayerContainer::stopChannel(const QString& channelKey) {
    PlaybackGroup* group = m_channelGroups.value(channelKey, nullptr);
    if (!group) {
        return;
    }
    group->stop();
}

void ReplayPlayerContainer::setChannelSpeed(const QString& channelKey, double speed) {
    PlaybackGroup* group = m_channelGroups.value(channelKey, nullptr);
    if (!group) {
        return;
    }
    m_controller->setGroupSpeed(group, speed);
}

// ── 拖放：录像文件拖到播放窗口 ──────────────────────────────────────

void ReplayPlayerContainer::dragEnterEvent(QDragEnterEvent* event) {
    if (event->mimeData()->hasFormat("application/x-rustcam-recording")
        || event->mimeData()->hasFormat(kDeviceMimeType)) {
        event->acceptProposedAction();
    }
}

void ReplayPlayerContainer::dragMoveEvent(QDragMoveEvent* event) {
    if (event->mimeData()->hasFormat("application/x-rustcam-recording")
        || event->mimeData()->hasFormat(kDeviceMimeType)) {
        event->acceptProposedAction();
    }
}

void ReplayPlayerContainer::dropEvent(QDropEvent* event) {
    // 找到落点所在的播放窗口
    const QPoint pos = m_videoArea->mapFrom(this, event->pos());
    QString cellId;
    for (auto it = m_cells.constBegin(); it != m_cells.constEnd(); ++it) {
        if (it.value()->geometry().contains(pos)) {
            cellId = it.key();
            break;
        }
    }
    if (cellId.isEmpty()) {
        return;   // 落在窗口外：忽略
    }

    if (event->mimeData()->hasFormat(kDeviceMimeType)) {
        const QJsonObject obj = QJsonDocument::fromJson(
            event->mimeData()->data(kDeviceMimeType)).object();
        if (obj["type"].toString() != "channel") {
            return;
        }
        const QString deviceTag = obj["device_tag"].toString();
        const QString channelTag = obj["channel_tag"].toString();
        if (deviceTag.isEmpty() || channelTag.isEmpty()) {
            return;
        }
        emit channelDropped(cellId, deviceTag, channelTag, obj["name"].toString());
        event->acceptProposedAction();
        return;
    }

    if (!event->mimeData()->hasFormat("application/x-rustcam-recording")) {
        return;
    }
    const QJsonObject obj = QJsonDocument::fromJson(
        event->mimeData()->data("application/x-rustcam-recording")).object();
    const QString channelKey = obj["channel_key"].toString();
    const int64_t startTimeMs = static_cast<int64_t>(obj["start_time_ms"].toDouble());
    if (channelKey.isEmpty()) {
        return;
    }
    emit recordingDropped(cellId, channelKey, startTimeMs);
    event->acceptProposedAction();
}

void ReplayPlayerContainer::resizeEvent(QResizeEvent* event) {
    QWidget::resizeEvent(event);
    if (!m_currentLayout.layoutJson.isEmpty() && m_videoArea && m_videoArea->width() > 0 && m_videoArea->height() > 0) {
        calculateLayout(m_currentLayout);
    }
}

bool ReplayPlayerContainer::eventFilter(QObject* watched, QEvent* event) {
    if (watched == m_videoArea && event->type() == QEvent::Resize) {
        if (!m_currentLayout.layoutJson.isEmpty()) {
            calculateLayout(m_currentLayout);
        }
    }
    return QWidget::eventFilter(watched, event);
}

}
