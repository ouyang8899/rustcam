#pragma once

#include <QWidget>
#include <QList>
#include <QMap>
#include <QPainter>
#include "CoreExport.h"

namespace rustcam {

struct RecordingPeriod {
    int64_t startTimeMs;
    int64_t endTimeMs;
    QString fileId;
    QString url;
};

class RUSTCAM_CORE_API TimelineWidget : public QWidget {
    Q_OBJECT
public:
    explicit TimelineWidget(QWidget* parent = nullptr);
    ~TimelineWidget();

    void setTimeRange(int64_t startMs, int64_t endMs);
    void setPeriods(const QList<RecordingPeriod>& periods);
    void setCurrentTime(int64_t timeMs);
    int64_t currentTime() const { return m_currentTimeMs; }

    void setDayChangeCallback(std::function<void(const QDate&)> cb);

signals:
    void timeSelected(int64_t timeMs);
    void periodClicked(const RecordingPeriod& period);

protected:
    void paintEvent(QPaintEvent* event) override;
    void mousePressEvent(QMouseEvent* event) override;
    void resizeEvent(QResizeEvent* event) override;

private:
    int64_t timeToX(int64_t timeMs) const;
    int64_t xToTime(int x) const;
    void updateLayout();

    int64_t m_startTimeMs = 0;
    int64_t m_endTimeMs = 0;
    int64_t m_currentTimeMs = 0;
    QList<RecordingPeriod> m_periods;
    QList<QRect> m_periodRects;
    static const int margin = 4;

    std::function<void(const QDate&)> m_dayChangeCallback;
};

}
