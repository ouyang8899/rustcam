#include "TimelineWidget.h"
#include <QPainter>
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

void TimelineWidget::setCurrentTime(int64_t timeMs) {
    if (m_currentTimeMs != timeMs) {
        m_currentTimeMs = timeMs;
        update();
    }
}

int64_t TimelineWidget::timeToX(int64_t timeMs) const {
    if (m_endTimeMs <= m_startTimeMs) return 0;
    double ratio = double(timeMs - m_startTimeMs) / double(m_endTimeMs - m_startTimeMs);
    return int(margin + ratio * (width() - margin * 2));
}

int64_t TimelineWidget::xToTime(int x) const {
    if (width() <= margin * 2) return m_startTimeMs;
    double ratio = double(x - margin) / double(width() - margin * 2);
    return m_startTimeMs + ratio * (m_endTimeMs - m_startTimeMs);
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

    QColor hasRecordingColor("#2d5a3d");
    for (const QRect& r : m_periodRects) {
        p.fillRect(r, hasRecordingColor);
    }

    QColor playheadColor("#ff6b35");
    int playheadX = timeToX(m_currentTimeMs);
    p.setPen(playheadColor);
    p.drawLine(playheadX, 0, playheadX, height());

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
}

void TimelineWidget::mousePressEvent(QMouseEvent* event) {
    if (event->button() == Qt::LeftButton) {
        int64_t time = xToTime(event->x());
        emit timeSelected(time);

        for (int i = 0; i < m_periodRects.size(); ++i) {
            if (m_periodRects[i].contains(event->pos())) {
                emit periodClicked(m_periods[i]);
                break;
            }
        }
    }
}

void TimelineWidget::resizeEvent(QResizeEvent* event) {
    QWidget::resizeEvent(event);
    updateLayout();
}

}
