#include "ReplayPlayerContainer.h"
#include "PlaybackCell.h"
#include "PlaybackController.h"
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
#include <QtConcurrent>
#include <QFutureWatcher>
#include <QMessageBox>

namespace rustcam {

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
    m_videoArea->setStyleSheet(R"(
        QWidget#videoArea {
            background-color: #1a1a1a;
        }
    )");

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

    QList<PlaybackCell*> oldCells = m_cells.values();
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
            connect(cell, &PlaybackCell::clicked, this, &ReplayPlayerContainer::onCellClicked);
            connect(cell, &PlaybackCell::playerDropped, this,
                    [this, cell](const QString& sourceUid) {
                        onPlayerDropped(cell, sourceUid);
                    });
            connect(cell, &PlaybackCell::recordingDropped, this,
                    [this, cell](const QString& channelKey, int64_t startTimeMs) {
                        emit recordingDropped(channelKey, startTimeMs, cell->layoutItemId());
                    });
            cell->setPlayerWidget(new PlaybackPlayerWidget(cell));
            newCount++;
        }

        cell->setCellId(cellIndex);
        cell->setObjectName(QString("PlaybackCell_%1").arg(item.id));
        // 格子在布局中的位置标识
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
    for (int i = 0; i < layout.layoutJson.size(); ++i) {
        m_cells.insert(layout.layoutJson[i].id, cellsToKeep[i]);
    }

    setupTimelineConnections();

    LOG_INFO("ReplayPlayerContainer", QString("calculateLayout: reused %1, created %2, deleted %3")
        .arg(reusedCount).arg(newCount).arg(oldCells.size() - reusedCount));
}

void ReplayPlayerContainer::onPlayerDropped(PlaybackCell* target, const QString& sourceUid) {
    PlaybackCell* source = findCellByUid(sourceUid);
    if (!source || !target || source == target) {
        return;
    }
    // 两个格子里的播放器对调：通道绑定跟着播放器走，不用重新绑定
    PlaybackPlayerWidget* sourcePlayer = source->takePlayerWidget();
    PlaybackPlayerWidget* targetPlayer = target->takePlayerWidget();
    source->setPlayerWidget(targetPlayer);
    target->setPlayerWidget(sourcePlayer);
    // 播放器换了格子，进度上报要按新格子来
    setupTimelineConnections();
    LOG_INFO("ReplayPlayerContainer", QString("swap players: %1 <-> %2")
        .arg(source->layoutItemId(), target->layoutItemId()));
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

void ReplayPlayerContainer::onCellClicked() {
    PlaybackCell* cell = qobject_cast<PlaybackCell*>(sender());
    QString oldSelected = m_selectedCellId;
    if (cell) {
        m_selectedCellId = cell->layoutItemId();

        if (!oldSelected.isEmpty()) {
            PlaybackCell* oldCell = m_cells.value(oldSelected);
            if (oldCell) oldCell->setSelected(false);
        }

        cell->setSelected(true);
        PlaybackPlayerWidget* player = cell->playerWidget();
        emit cellClicked(cell->cellId(), player ? player->deviceId() : QString());
    }

    if (m_selectedCellId != oldSelected) {
        emit targetCellChanged(m_selectedCellId);
    }
}

void ReplayPlayerContainer::onCellProgressChanged(const QString& cellId, int64_t timeMs)
{
    emit playbackProgress(timeMs);
    // 播放头画在播放器所属通道的时间轴上
    PlaybackCell* cell = m_cells.value(cellId, nullptr);
    PlaybackPlayerWidget* player = cell ? cell->playerWidget() : nullptr;
    const QString channelKey = player ? player->channelKey() : QString();
    if (!channelKey.isEmpty()) {
        emit channelPlaybackProgress(channelKey, timeMs);
    }
    TimelineWidget* timeline = channelKey.isEmpty() ? activeTimeline() : channelTimeline(channelKey);
    if (timeline && player) {
        timeline->updatePlayhead(player->uid(), timeMs);
    }
}

void ReplayPlayerContainer::setupTimelineConnections() {
    for (auto it = m_cells.begin(); it != m_cells.end(); ++it) {
        QString cellId = it.key();
        PlaybackCell* cell = it.value();
        if (!cell || !m_controller) {
            continue;
        }
        PlaybackPlayerWidget* player = cell->playerWidget();
        if (!player) {
            continue;
        }
        // 布局重算会重复调用这里，先断开旧连接，避免一次进度触发多次回调
        disconnect(player, &PlaybackPlayerWidget::playbackProgressChanged, this, nullptr);
        connect(player, &PlaybackPlayerWidget::playbackProgressChanged, this, [this, cellId](int64_t timeMs) {
            onCellProgressChanged(cellId, timeMs);
        });
    }
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

    // 时间轴上的定位、分段点击由通道自己转发（PlaybackChannel -> seekRequested）
    // 这里只处理拖拽播放头：松手后才 seek，避免拖动过程中反复重建播放器
    connect(timeline, &TimelineWidget::playheadReleased, this,
            [this, channelKey](const QString& windowId, int64_t timeMs) {
                Q_UNUSED(windowId)
                seekChannel(channelKey, timeMs);
            });

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

// ── 拖放：录像文件拖到播放窗口 ──────────────────────────────────────

void ReplayPlayerContainer::dragEnterEvent(QDragEnterEvent* event) {
    if (event->mimeData()->hasFormat("application/x-rustcam-recording")) {
        event->acceptProposedAction();
    }
}

void ReplayPlayerContainer::dragMoveEvent(QDragMoveEvent* event) {
    if (event->mimeData()->hasFormat("application/x-rustcam-recording")) {
        event->acceptProposedAction();
    }
}

void ReplayPlayerContainer::dropEvent(QDropEvent* event) {
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

    // 找到落点所在的播放窗口
    const QPoint pos = event->pos();
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
    emit recordingDropped(channelKey, startTimeMs, cellId);
    event->acceptProposedAction();
}

void ReplayPlayerContainer::play() {
    if (m_controller) {
        m_controller->play();
    }
}

void ReplayPlayerContainer::pause() {
    if (m_controller) {
        m_controller->pause();
    }
}

void ReplayPlayerContainer::stop() {
    if (m_controller) {
        m_controller->stop();
    }
}

void ReplayPlayerContainer::setSpeed(double speed) {
    if (m_controller) {
        m_controller->setSpeed(speed);
    }
}

void ReplayPlayerContainer::resizeEvent(QResizeEvent* event) {
    QWidget::resizeEvent(event);
    if (!m_currentLayout.layoutJson.isEmpty() && m_videoArea && m_videoArea->width() > 0 && m_videoArea->height() > 0) {
        calculateLayout(m_currentLayout);
    }
}

}
