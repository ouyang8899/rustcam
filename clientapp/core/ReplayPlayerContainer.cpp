#include "ReplayPlayerContainer.h"
#include "PlaybackCell.h"
#include "PlaybackController.h"
#include "ReplayNavBar.h"
#include "ServerApi.h"
#include "common/Logger.h"
#include <QVBoxLayout>
#include <QJsonArray>
#include <QtConcurrent>
#include <QFutureWatcher>

namespace rustcam {

ReplayPlayerContainer::ReplayPlayerContainer(QWidget* parent)
    : QWidget(parent)
    , m_videoArea(nullptr)
    , m_navBar(nullptr)
    , m_controller(nullptr)
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

    m_navBar = new ReplayNavBar(this);
    m_navBar->setObjectName("navBar");

    m_controller = new PlaybackController(this);

    layout->addWidget(m_videoArea, 1);
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

void ReplayPlayerContainer::loadRecordings(const QString& deviceId, int64_t startTimeMs, int64_t endTimeMs) {
    QString endpoint = QString("/api/recordings/files?device_id=%1&start_time=%2&end_time=%3")
        .arg(deviceId)
        .arg(startTimeMs)
        .arg(endTimeMs);

    QFuture<QPair<bool, QJsonObject>> future = QtConcurrent::run([endpoint]() {
        QJsonObject response;
        QString error;
        bool success = ServerApi::instance()->getSync(endpoint, response, error);
        Q_UNUSED(error)
        return QPair<bool, QJsonObject>(success, response);
    });

    QFutureWatcher<QPair<bool, QJsonObject>>* watcher = new QFutureWatcher<QPair<bool, QJsonObject>>(this);
    connect(watcher, &QFutureWatcher<QPair<bool, QJsonObject>>::finished, [this, watcher, deviceId]() {
        QPair<bool, QJsonObject> result = watcher->result();
        onRecordingsReceived(result.first, result.second, QString(), deviceId);
        watcher->deleteLater();
    });
    watcher->setFuture(future);
}

void ReplayPlayerContainer::onRecordingsReceived(bool success, const QJsonObject& response, const QString& error, const QString& deviceId) {
    if (!success) {
        LOG_ERROR("ReplayPlayerContainer", QString("loadRecordings failed: %1").arg(error));
        return;
    }

    QList<Clip> clips;
    QJsonArray clipsArray = response["clips"].toArray();
    for (const QJsonValue& v : clipsArray) {
        QJsonObject obj = v.toObject();
        Clip clip;
        clip.url = obj["url"].toString();
        clip.startTimeMs = obj["start_time"].toVariant().toLongLong();
        clip.endTimeMs = obj["end_time"].toVariant().toLongLong();
        clip.durationMs = clip.endTimeMs - clip.startTimeMs;
        clips.append(clip);
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

    LOG_INFO("ReplayPlayerContainer", QString("onRecordingsReceived: deviceId=%1, clipCount=%2").arg(deviceId).arg(clips.size()));
}

void ReplayPlayerContainer::calculateLayout(const PlayerLayout& layout) {
    for (PlaybackCell* cell : m_cells.values()) {
        delete cell;
    }
    m_cells.clear();

    const int margin = 2;
    const int spacing = 2;

    int availableW = m_videoArea->width();
    int availableH = m_videoArea->height();

    if (availableW <= 0 || availableH <= 0 || layout.cols <= 0 || layout.rows <= 0) {
        return;
    }

    int cw = (availableW - margin * 2 - spacing * (layout.cols - 1)) / layout.cols;
    int ch = (availableH - margin * 2 - spacing * (layout.rows - 1)) / layout.rows;

    int cellIndex = 0;
    for (const LayoutItem& item : layout.layoutJson) {
        int x = margin + item.col * (cw + spacing);
        int y = margin + item.row * (ch + spacing);
        int w = item.colSpan * cw + (item.colSpan - 1) * spacing;
        int h = item.rowSpan * ch + (item.rowSpan - 1) * spacing;

        PlaybackCell* cell = new PlaybackCell(m_videoArea);
        cell->setCellId(cellIndex);
        cell->setObjectName(QString("PlaybackCell_%1").arg(item.id));
        cell->setGeometry(x, y, w, h);

        connect(cell, &PlaybackCell::clicked, this, &ReplayPlayerContainer::onCellClicked);

        m_cells.insert(item.id, cell);
        cell->show();
        cellIndex++;
    }

    LOG_INFO("ReplayPlayerContainer", QString("calculateLayout: created %1 cells").arg(m_cells.size()));
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

void ReplayPlayerContainer::setBaseTime(int64_t timeMs) {
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
