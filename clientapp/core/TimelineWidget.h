#pragma once

#include <QWidget>
#include <QList>
#include <QMap>
#include <QPainter>
#include "CoreExport.h"

namespace rustcam {

enum class SegmentType {
    Normal,
    Alarm,
    PreBuffer
};

struct RecordingPeriod {
    int64_t startTimeMs;
    int64_t endTimeMs;
    QString fileId;
    QString url;
    SegmentType segmentType = SegmentType::Normal;
    int64_t alarmId = 0;
};

class RUSTCAM_CORE_API TimelineWidget : public QWidget {
    Q_OBJECT
public:
    struct Playhead {
        int64_t timeMs;
        QColor color;
    };

    explicit TimelineWidget(QWidget* parent = nullptr);
    ~TimelineWidget();

    void setTimeRange(int64_t startMs, int64_t endMs);
    void setPeriods(const QList<RecordingPeriod>& periods);

    void addPlayhead(const QString& windowId, const QColor& color);
    void removePlayhead(const QString& windowId);
    void updatePlayhead(const QString& windowId, int64_t timeMs);
    void clearAllPlayheads();

    void setCurrentTime(int64_t timeMs);
    int64_t currentTime() const { return m_mainPlayheadMs; }

    void setDayChangeCallback(std::function<void(const QDate&)> cb);

    void setZoom(double zoomLevel);
    double zoomLevel() const { return m_zoomLevel; }

    void setSelectedWindowId(const QString& windowId);
    QString selectedWindowId() const { return m_selectedWindowId; }

    signals:
        void playheadMoved(const QString& windowId, int64_t timeMs);
        void selectedWindowPlayheadMoved(const QString& windowId, int64_t timeMs);
        void timeSelected(int64_t timeMs);
        void periodClicked(const RecordingPeriod& period);
        void zoomChanged(double zoom);

protected:
    void paintEvent(QPaintEvent* event) override;
    void mousePressEvent(QMouseEvent* event) override;
    void mouseDoubleClickEvent(QMouseEvent* event) override;
    void mouseMoveEvent(QMouseEvent* event) override;
    void mouseReleaseEvent(QMouseEvent* event) override;
    void wheelEvent(QWheelEvent* event) override;
    void resizeEvent(QResizeEvent* event) override;

private:
    double maxScrollOffset() const;

private:
    int64_t timeToX(int64_t timeMs) const;
    int64_t xToTime(int x) const;
    void updateLayout();

    int64_t m_startTimeMs = 0;
    int64_t m_endTimeMs = 0;
    int64_t m_mainPlayheadMs = 0;
    QList<RecordingPeriod> m_periods;
    QList<QRect> m_periodRects;
    static const int margin = 4;

    QMap<QString, Playhead> m_playheads;

    std::function<void(const QDate&)> m_dayChangeCallback;

    double m_zoomLevel = 1.0;
    double m_scrollOffset = 0.0;
    bool m_isDragging = false;
    double m_dragStartX = 0;
    double m_dragStartOffset = 0;

    QString m_selectedWindowId;
    QString m_draggingPlayheadId;
    bool m_isDraggingPlayhead = false;
};

}
