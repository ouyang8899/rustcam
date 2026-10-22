#pragma once

#include <QWidget>
#include <QMap>
#include <QString>
#include <QJsonObject>
#include <QResizeEvent>
#include "CoreExport.h"
#include "PlayerLayout.h"
#include "DataManager.h"
#include "TimelineWidget.h"

namespace rustcam {

class PlaybackCell;
class PlaybackController;
class ReplayNavBar;

class RUSTCAM_CORE_API ReplayPlayerContainer : public QWidget {
    Q_OBJECT
public:
    explicit ReplayPlayerContainer(QWidget* parent = nullptr);
    ~ReplayPlayerContainer();

    void initialize();
    void loadRecordings(const QString& deviceId, int64_t startTimeMs, int64_t endTimeMs);
    void setBaseTime(int64_t timeMs);

    PlaybackCell* getCell(const QString& cellId) const;
    QList<QString> getCellIds() const { return m_cells.keys(); }
    PlaybackController* controller() const { return m_controller; }

public slots:
    void play();
    void pause();
    void stop();
    void setSpeed(double speed);

signals:
    void cellClicked(int cellId, const QString& deviceId);

private slots:
    void onLayoutsReady();
    void onLayoutSelected(int index);
    void onRecordingsReceived(bool success, const QJsonObject& response, const QString& error, const QString& deviceId, qint64 loadStartTimeMs = 0);
    void onCellClicked();
    void onTimeSelected(int64_t timeMs);
    void onCellProgressChanged(const QString& cellId, int64_t timeMs);

private:
    void calculateLayout(const PlayerLayout& layout);
    QString selectTargetCell();
    void setupTimelineConnections();

protected:
    void resizeEvent(QResizeEvent* event) override;

    QWidget* m_videoArea;
    ReplayNavBar* m_navBar;
    TimelineWidget* m_timeline;
    QList<PlayerLayout> m_layouts;
    PlayerLayout m_currentLayout;
    QMap<QString, PlaybackCell*> m_cells;
    QString m_selectedCellId;
    PlaybackController* m_controller;
    int m_totalRecords;
    int m_page;
    int m_limit;
    bool m_hasMore;
};

}
