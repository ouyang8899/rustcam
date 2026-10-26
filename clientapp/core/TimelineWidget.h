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

/// 时间轴上的事件标记（例如报警）
struct TimelineEvent {
    int64_t timeMs = 0;
    QColor color = QColor("#ff6b6b");
    QString label;
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
    /// 事件标记（报警等），画在录像段下方
    void setEvents(const QList<TimelineEvent>& events);
    const QList<TimelineEvent>& events() const { return m_events; }

    /// 书签标记
    void setBookmarks(const QList<TimelineEvent>& bookmarks);
    const QList<TimelineEvent>& bookmarks() const { return m_bookmarks; }

    void addPlayhead(const QString& windowId, const QColor& color);
    void removePlayhead(const QString& windowId);
    void updatePlayhead(const QString& windowId, int64_t timeMs);
    void clearAllPlayheads();
    const QMap<QString, Playhead>& playheads() const { return m_playheads; }

    void setCurrentTime(int64_t timeMs);
    int64_t currentTime() const { return m_mainPlayheadMs; }

    void setDayChangeCallback(std::function<void(const QDate&)> cb);

    void setZoom(double zoomLevel);
    double zoomLevel() const { return m_zoomLevel; }

    void setSelectedWindowId(const QString& windowId);
    QString selectedWindowId() const { return m_selectedWindowId; }

    signals:
        void playheadMoved(const QString& windowId, int64_t timeMs);
        /// 拖拽播放头结束（拖动过程中只移动标记，松手才 seek，避免连续切换片源）
        void playheadReleased(const QString& windowId, int64_t timeMs);
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
    QList<TimelineEvent> m_events;
    QList<TimelineEvent> m_bookmarks;
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
    /// 左键按下后等待松开时定位
    QPoint m_pressPos;
    bool m_pendingSeek = false;
};

}
