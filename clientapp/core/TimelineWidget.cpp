#include "TimelineWidget.h"
#include <QPainter>
#include <QPainterPath>
#include <QMouseEvent>
#include <QDateTime>
#include <QDebug>

namespace rustcam {

TimelineWidget::TimelineWidget(QWidget* parent)
    : QWidget(parent)
{
    setMinimumHeight(60);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
    setStyleSheet("background-color: #1e1e1e; border: 1px solid #333;");
}

TimelineWidget::~TimelineWidget() {
}

void TimelineWidget::setSelectedWindowId(const QString& windowId) {
    m_selectedWindowId = windowId;
}

void TimelineWidget::setTimeRange(int64_t startMs, int64_t endMs) {
    m_startTimeMs = startMs;
    m_endTimeMs = endMs;
    updateLayout();
    update();
}

void TimelineWidget::setPeriods(const QList<RecordingPeriod>& periods) {
    m_periods = periods;
    updateLayout();
    update();
}

void TimelineWidget::addPlayhead(const QString& windowId, const QColor& color) {
    m_playheads[windowId] = {m_mainPlayheadMs, color};
    update();
}

void TimelineWidget::removePlayhead(const QString& windowId) {
    m_playheads.remove(windowId);
    update();
}

void TimelineWidget::updatePlayhead(const QString& windowId, int64_t timeMs) {
    if (m_playheads.contains(windowId)) {
        m_playheads[windowId].timeMs = timeMs;
        update();
    }
}

void TimelineWidget::clearAllPlayheads() {
    m_playheads.clear();
    update();
}

void TimelineWidget::setCurrentTime(int64_t timeMs) {
    if (m_mainPlayheadMs != timeMs) {
        m_mainPlayheadMs = timeMs;
        update();
    }
}

void TimelineWidget::setZoom(double zoomLevel) {
    if (m_zoomLevel != zoomLevel) {
        double maxOffset = maxScrollOffset();
        if (m_scrollOffset > maxOffset) {
            m_scrollOffset = maxOffset;
        }
        m_zoomLevel = zoomLevel;
        updateLayout();
        update();
        emit zoomChanged(zoomLevel);
    }
}

int64_t TimelineWidget::timeToX(int64_t timeMs) const {
    if (m_endTimeMs <= m_startTimeMs) return margin;
    double contentWidth = (width() - margin * 2) * m_zoomLevel;
    double ratio = double(timeMs - m_startTimeMs) / double(m_endTimeMs - m_startTimeMs);
    double x = ratio * contentWidth - m_scrollOffset;
    return int(margin + x);
}

int64_t TimelineWidget::xToTime(int x) const {
    double contentWidth = (width() - margin * 2) * m_zoomLevel;
    if (contentWidth <= 0) return m_startTimeMs;
    double adjustedX = x - margin + m_scrollOffset;
    double ratio = adjustedX / contentWidth;
    int64_t time = m_startTimeMs + ratio * (m_endTimeMs - m_startTimeMs);
    return qBound(m_startTimeMs, time, m_endTimeMs);
}

void TimelineWidget::updateLayout() {
    m_periodRects.clear();
    const int barHeight = 30;
    const int barY = (height() - barHeight) / 2;

    for (const RecordingPeriod& period : m_periods) {
        int x1 = timeToX(period.startTimeMs);
        int x2 = timeToX(period.endTimeMs);
        m_periodRects.append(QRect(x1, barY, x2 - x1, barHeight));
    }
}

void TimelineWidget::paintEvent(QPaintEvent* event) {
    QWidget::paintEvent(event);
    QPainter p(this);

    p.setRenderHint(QPainter::Antialiasing);

    p.fillRect(rect(), QColor("#1e1e1a"));

    QColor noRecordingColor("#2a2a2a");
    p.fillRect(QRect(margin, 0, width() - margin * 2, height()), noRecordingColor);

    int visibleLeft = margin;
    int visibleRight = width() - margin;

    struct MergedRect {
        SegmentType type;
        QRect rect;
    };
    QList<MergedRect> mergedRects;

    for (int i = 0; i < m_periods.size(); ++i) {
        const QRect& r = m_periodRects[i];

        if (r.right() < visibleLeft) continue;
        if (r.left() > visibleRight) break;

        const RecordingPeriod& period = m_periods[i];

        if (!mergedRects.isEmpty() && mergedRects.last().type == period.segmentType) {
            MergedRect& last = mergedRects.last();
            QRect combined = last.rect.united(r);
            last.rect = combined;
        } else {
            mergedRects.append({period.segmentType, r});
        }
    }

    QPainterPath pathNormal;
    QPainterPath pathAlarm;
    QPainterPath pathPreBuffer;

    for (const MergedRect& mr : mergedRects) {
        QPainterPath* path = nullptr;
        switch (mr.type) {
            case SegmentType::Normal:
                path = &pathNormal;
                break;
            case SegmentType::Alarm:
                path = &pathAlarm;
                break;
            case SegmentType::PreBuffer:
                path = &pathPreBuffer;
                break;
        }
        if (path) {
            path->addRect(mr.rect);
        }
    }

    p.fillPath(pathNormal, QColor("#4A90D9"));
    p.fillPath(pathAlarm, QColor("#E74C3C"));
    p.fillPath(pathPreBuffer, QColor("#87CEEB"));

    for (auto it = m_playheads.begin(); it != m_playheads.end(); ++it) {
        const Playhead& ph = it.value();
        int playheadX = timeToX(ph.timeMs);
        p.setPen(ph.color);
        p.drawLine(playheadX, 0, playheadX, height());
    }

    p.setPen(QColor("#888"));
    QFont font = p.font();
    font.setPointSize(8);
    p.setFont(font);

    QDateTime startDt = QDateTime::fromMSecsSinceEpoch(m_startTimeMs);
    QDateTime endDt = QDateTime::fromMSecsSinceEpoch(m_endTimeMs);
    QString startStr = startDt.toString("HH:mm");
    QString endStr = endDt.toString("HH:mm");

    p.drawText(QPoint(margin, 12), startStr);
    p.drawText(QPoint(width() - margin - 30, 12), endStr);

    int dayOffset = (startDt.date().dayOfWeek() - 1) * (width() - margin * 2) / 7;
    p.drawText(QPoint(margin + dayOffset, 12), startDt.toString("yyyy-MM-dd"));

    p.setPen(QPen(Qt::white, 1));

    int tickCount = 10;
    int64_t timeRange = m_endTimeMs - m_startTimeMs;

    for (int i = 0; i <= tickCount; i++) {
        int64_t time = m_startTimeMs + (timeRange * i) / tickCount;
        int x = timeToX(time);

        p.drawLine(x, height() - 20, x, height());

        QDateTime dt = QDateTime::fromMSecsSinceEpoch(time);
        QString timeStr = dt.toString("HH:mm");
        p.drawText(x - 15, height() - 5, timeStr);
    }
}

void TimelineWidget::mousePressEvent(QMouseEvent* event) {
    if (event->button() == Qt::MiddleButton ||
        (event->button() == Qt::LeftButton && event->modifiers() & Qt::ShiftModifier)) {
        m_isDragging = true;
        m_dragStartX = event->x();
        m_dragStartOffset = m_scrollOffset;
    }
}

void TimelineWidget::mouseDoubleClickEvent(QMouseEvent* event) {
    if (event->button() == Qt::LeftButton) {
        for (auto it = m_playheads.begin(); it != m_playheads.end(); ++it) {
            int playheadX = timeToX(it.value().timeMs);
            if (qAbs(event->x() - playheadX) < 15) {
                m_draggingPlayheadId = it.key();
                m_isDraggingPlayhead = true;
                return;
            }
        }

        int64_t clickedTime = xToTime(event->x());

        if (clickedTime < m_startTimeMs) clickedTime = m_startTimeMs;
        if (clickedTime > m_endTimeMs) clickedTime = m_endTimeMs;

        emit timeSelected(clickedTime);

        if (!m_selectedWindowId.isEmpty() && m_playheads.contains(m_selectedWindowId)) {
            m_playheads[m_selectedWindowId].timeMs = clickedTime;
            emit selectedWindowPlayheadMoved(m_selectedWindowId, clickedTime);
        }

        for (int i = 0; i < m_periodRects.size(); ++i) {
            if (m_periodRects[i].contains(event->pos())) {
                emit periodClicked(m_periods[i]);
                break;
            }
        }
        update();
    }
}

void TimelineWidget::mouseMoveEvent(QMouseEvent* event) {
    if (m_isDragging) {
        int deltaX = int(m_dragStartX) - int(event->x());
        double newOffset = m_dragStartOffset + deltaX;
        m_scrollOffset = qBound(0.0, newOffset, maxScrollOffset());
        update();
        return;
    }

    if (m_isDraggingPlayhead) {
        int64_t time = xToTime(event->x());
        time = qBound(m_startTimeMs, time, m_endTimeMs);
        if (m_playheads.contains(m_draggingPlayheadId)) {
            m_playheads[m_draggingPlayheadId].timeMs = time;
            emit playheadMoved(m_draggingPlayheadId, time);
            update();
        }
        return;
    }

    for (const RecordingPeriod& period : m_periods) {
        int xFrom = timeToX(period.startTimeMs);
        int xTo = timeToX(period.endTimeMs);

        if (event->x() >= xFrom && event->x() <= xTo) {
            QDateTime start = QDateTime::fromMSecsSinceEpoch(period.startTimeMs);
            QDateTime end = QDateTime::fromMSecsSinceEpoch(period.endTimeMs);

            QString tip = QString("%1 - %2\n类型: %3")
                .arg(start.toString("HH:mm:ss"))
                .arg(end.toString("HH:mm:ss"))
                .arg(period.segmentType == SegmentType::Alarm ? "报警" : "普通");

            setToolTip(tip);
            return;
        }
    }

    setToolTip(QString());
}

void TimelineWidget::resizeEvent(QResizeEvent* event) {
    QWidget::resizeEvent(event);
    updateLayout();
}

void TimelineWidget::mouseReleaseEvent(QMouseEvent* event) {
    if (event->button() == Qt::MiddleButton ||
        (event->button() == Qt::LeftButton && event->modifiers() & Qt::ShiftModifier)) {
        m_isDragging = false;
    }

    if (event->button() == Qt::LeftButton) {
        m_isDraggingPlayhead = false;
        m_draggingPlayheadId.clear();
    }
}

void TimelineWidget::wheelEvent(QWheelEvent* event) {
    if (event->angleDelta().y() > 0) {
        setZoom(m_zoomLevel * 1.2);
    } else {
        setZoom(m_zoomLevel / 1.2);
    }
}

double TimelineWidget::maxScrollOffset() const {
    double contentWidth = (width() - margin * 2) * m_zoomLevel;
    double maxOffset = contentWidth - (width() - margin * 2);
    return qMax(0.0, maxOffset);
}

}
