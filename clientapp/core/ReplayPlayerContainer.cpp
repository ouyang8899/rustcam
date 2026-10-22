#include "ReplayPlayerContainer.h"
#include "PlaybackCell.h"
#include "PlaybackController.h"
#include "ReplayNavBar.h"
#include "ServerApi.h"
#include "common/Logger.h"
#include <QVBoxLayout>
#include <QJsonArray>
#include <QJsonObject>
#include <QDateTime>
#include <QtConcurrent>
#include <QFutureWatcher>
#include <QMessageBox>

namespace rustcam {

ReplayPlayerContainer::ReplayPlayerContainer(QWidget* parent)
    : QWidget(parent)
    , m_videoArea(nullptr)
    , m_navBar(nullptr)
    , m_timeline(nullptr)
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

    m_timeline = new TimelineWidget(this);
    m_timeline->setObjectName("timeline");

    m_navBar = new ReplayNavBar(this);
    m_navBar->setObjectName("navBar");

    m_controller = new PlaybackController(this);

    layout->addWidget(m_videoArea, 1);
    layout->addWidget(m_timeline, 0);
    layout->addWidget(m_navBar, 0);

    connect(m_navBar, &ReplayNavBar::layoutSelected, this, &ReplayPlayerContainer::onLayoutSelected);
    connect(m_timeline, &TimelineWidget::timeSelected, this, &ReplayPlayerContainer::onTimeSelected);
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

void ReplayPlayerContainer::loadRecordings(const QString& deviceId, int64_t startTimeMs, int64_t endTimeMs) {
    LOG_INFO("ReplayPlayerContainer", QString("loadRecordings: deviceId=%1, startTimeMs=%2, endTimeMs=%3").arg(deviceId).arg(startTimeMs).arg(endTimeMs));

    QDateTime startDt = QDateTime::fromMSecsSinceEpoch(startTimeMs);
    QDateTime endDt = QDateTime::fromMSecsSinceEpoch(endTimeMs);

    QString endpoint = QString("/api/v1/recordings/files?device_tag=%1&start_time=%2&end_time=%3&limit=50&page=1")
        .arg(deviceId)
        .arg(startDt.toString(Qt::ISODate))
        .arg(endDt.toString(Qt::ISODate));

    QFuture<QPair<bool, QJsonObject>> future = QtConcurrent::run([endpoint]() {
        QJsonObject response;
        QString error;
        bool success = ServerApi::instance()->getSync(endpoint, response, error);
        Q_UNUSED(error)
        return QPair<bool, QJsonObject>(success, response);
    });

    qint64 loadStartTimeMs = QDateTime::currentMSecsSinceEpoch();
    QFutureWatcher<QPair<bool, QJsonObject>>* watcher = new QFutureWatcher<QPair<bool, QJsonObject>>(this);
    connect(watcher, &QFutureWatcher<QPair<bool, QJsonObject>>::finished, [this, watcher, deviceId, loadStartTimeMs]() {
        QPair<bool, QJsonObject> result = watcher->result();
        onRecordingsReceived(result.first, result.second, QString(), deviceId, loadStartTimeMs);
        watcher->deleteLater();
    });
    watcher->setFuture(future);
}

void ReplayPlayerContainer::onRecordingsReceived(bool success, const QJsonObject& response, const QString& error, const QString& deviceId, qint64 loadStartTimeMs) {
    if (!success) {
        LOG_ERROR("ReplayPlayerContainer", QString("onRecordingsReceived failed: deviceId=%1, error=%2").arg(deviceId).arg(error));

        QMessageBox::warning(this, tr("加载失败"),
            tr("无法加载录像文件：%1").arg(error));
        return;
    }

    QList<Clip> clips;
    QJsonObject dataObj = response["data"].toObject();
    QJsonArray clipsArray = dataObj["files"].toArray();

    m_totalRecords = dataObj["total"].toInt(0);
    m_page = dataObj["page"].toInt(1);
    m_limit = dataObj["limit"].toInt(50);
    m_hasMore = dataObj["has_more"].toBool(false);

    for (const QJsonValue& v : clipsArray) {
        QJsonObject obj = v.toObject();

        QString startTimeStr = obj["start_time"].toString();
        QString endTimeStr = obj["end_time"].toString();
        QDateTime startDt = QDateTime::fromString(startTimeStr, Qt::ISODate);
        QDateTime endDt = QDateTime::fromString(endTimeStr, Qt::ISODate);
        int64_t startTimeMs = startDt.toMSecsSinceEpoch();
        int64_t endTimeMs = endDt.toMSecsSinceEpoch();

        Clip clip;
        clip.url = obj["play_url"].toString();
        clip.startTimeMs = startTimeMs;
        clip.endTimeMs = endTimeMs;
        clip.durationMs = obj["duration_secs"].toVariant().toLongLong() * 1000;

        QString segmentTypeStr = obj["segment_type"].toString().toLower();
        if (segmentTypeStr == "alarm") {
            clip.segmentType = SegmentType::Alarm;
        } else if (segmentTypeStr == "prebuffer" || segmentTypeStr == "pre-buffer") {
            clip.segmentType = SegmentType::PreBuffer;
        } else {
            clip.segmentType = SegmentType::Normal;
        }

        clip.alarmId = obj["alarm_id"].toVariant().toLongLong();

        clips.append(clip);
    }

    LOG_INFO("ReplayPlayerContainer", QString("onRecordingsReceived: deviceId=%1, fileCount=%2, hasMore=%3").arg(deviceId).arg(clips.size()).arg(m_hasMore));

    qint64 elapsed = QDateTime::currentMSecsSinceEpoch() - loadStartTimeMs;
    LOG_INFO("ReplayPlayerContainer",
        QString("loadRecordings completed in %1ms, fileCount=%2")
            .arg(elapsed)
            .arg(clips.size()));

    if (elapsed > 1000) {
        LOG_WARNING("ReplayPlayerContainer", "Load time > 1s, consider optimization");
    }

    if (clips.isEmpty()) {
        LOG_INFO("ReplayPlayerContainer", QString("onRecordingsReceived: no clips found for device %1").arg(deviceId));
        return;
    }

    QString targetCellId = selectTargetCell();
    PlaybackCell* cell = m_cells.value(targetCellId);
    if (cell) {
        cell->bindDevice(deviceId);
        cell->loadClips(clips);
        cell->setController(m_controller);
        cell->setGroup(m_controller->defaultGroup());
    }
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
            newCount++;
        }

        cell->setCellId(cellIndex);
        cell->setObjectName(QString("PlaybackCell_%1").arg(item.id));
        cell->setGeometry(x, y, w, h);
        cell->show();
        cellsToKeep.append(cell);
        cellIndex++;
    }

    for (int i = cellsToKeep.size(); i < oldCells.size(); ++i) {
        delete oldCells[i];
    }

    m_cells.clear();
    for (int i = 0; i < layout.layoutJson.size(); ++i) {
        m_cells.insert(layout.layoutJson[i].id, cellsToKeep[i]);
    }

    setupTimelineConnections();

    LOG_INFO("ReplayPlayerContainer", QString("calculateLayout: reused %1, created %2, deleted %3")
        .arg(reusedCount).arg(newCount).arg(oldCells.size() - reusedCount));
}

PlaybackCell* ReplayPlayerContainer::getCell(const QString& cellId) const {
    return m_cells.value(cellId, nullptr);
}

QString ReplayPlayerContainer::selectTargetCell() {
    if (!m_selectedCellId.isEmpty()) {
        PlaybackCell* selected = m_cells.value(m_selectedCellId);
        if (selected) {
            return m_selectedCellId;
        }
    }

    if (!m_cells.isEmpty()) {
        return m_cells.firstKey();
    }

    return QString();
}

void ReplayPlayerContainer::onCellClicked() {
    PlaybackCell* cell = qobject_cast<PlaybackCell*>(sender());
    if (cell) {
        QString oldSelected = m_selectedCellId;
        m_selectedCellId = cell->objectName();

        if (!oldSelected.isEmpty()) {
            PlaybackCell* oldCell = m_cells.value(oldSelected);
            if (oldCell) oldCell->setSelected(false);
        }

        cell->setSelected(true);
        emit cellClicked(cell->cellId(), cell->deviceId());
    }
}

void ReplayPlayerContainer::onTimeSelected(int64_t timeMs) {
    setBaseTime(timeMs);
}

void ReplayPlayerContainer::onCellProgressChanged(const QString& cellId, int64_t timeMs) {
    if (m_timeline) {
        m_timeline->updatePlayhead(cellId, timeMs);
    }
}

void ReplayPlayerContainer::setupTimelineConnections() {
    for (auto it = m_cells.begin(); it != m_cells.end(); ++it) {
        QString cellId = it.key();
        PlaybackCell* cell = it.value();
        if (cell && m_timeline) {
            m_timeline->addPlayhead(cellId, QColor("#4a9eff"));
            connect(cell, &PlaybackCell::playbackProgressChanged, this, [this, cellId](int64_t timeMs) {
                onCellProgressChanged(cellId, timeMs);
            });
        }
    }
}

void ReplayPlayerContainer::setBaseTime(int64_t timeMs) {
    LOG_DEBUG("ReplayPlayerContainer", QString("setBaseTime: timeMs=%1").arg(timeMs));
    if (m_controller) {
        m_controller->setBaseTime(timeMs);
    }
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
