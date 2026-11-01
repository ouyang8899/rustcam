#include "TimelineWidget.h"
#include "ThemeManager.h"
#include <QPainter>
#include <QPainterPath>
#include <QMouseEvent>
#include <QDateTime>
#include <QFontMetrics>
#include <algorithm>
#include <limits>

namespace rustcam {

namespace {

/// 顶部一行（加载范围 / 播放头实时读数）的文字基线
constexpr int kHeaderBaseline = 10;
/// 录像段条的顶部与高度
constexpr int kBarTop = 12;
constexpr int kBarHeight = 17;
/// 次刻度线：短
constexpr int kMinorTickTop = 30;
constexpr int kMinorTickHeight = 3;
/// 主刻度线：长
constexpr int kMajorTickTop = 30;
constexpr int kMajorTickHeight = 6;
/// 主刻度标签基线
constexpr int kLabelBaseline = 44;

/// 各类录像段的配色（取自当前主题）
QColor segmentColor(const ThemeManager* theme, SegmentType type) {
    switch (type) {
        case SegmentType::Alarm:
            return theme->getColor("error", QColor("#E74C3C"));
        case SegmentType::PreBuffer:
            return theme->getColor("info", QColor("#87CEEB"));
        case SegmentType::Normal:
        default:
            return theme->getColor("accent", QColor("#4A90D9"));
    }
}

}  // namespace

// ── 阶梯与刻度单位 ─────────────────────────────────────────────────────

QList<int64_t> TimelineWidget::tickLadder() {
    // 升序；最细 100ms（十分之一秒）
    return QList<int64_t>{
        100, 200, 500,
        1000, 2000, 5000, 10000, 15000, 30000,
        60000, 120000, 300000, 600000, 900000, 1800000,
        3600000, 7200000, 10800000, 21600000, 43200000,
        86400000, 172800000, 259200000, 604800000, 2592000000};
}

QList<int64_t> TimelineWidget::zoomLadder() {
    // 可见跨度阶梯（升序）；最细 1s
    return QList<int64_t>{
        1000, 2000, 5000, 10000, 15000, 30000,
        60000, 120000, 300000, 600000, 900000, 1800000,
        3600000, 7200000, 10800000, 21600000, 43200000,
        86400000, 172800000, 259200000, 604800000, 2592000000};
}

bool TimelineWidget::chooseTickUnits(double pxPerSecond, int64_t& majorMs, int64_t& minorMs) {
    majorMs = 0;
    minorMs = 0;
    if (pxPerSecond <= 0.0) {
        return false;
    }

    const QList<int64_t> ladder = tickLadder();

    // 主刻度：像素间距达标（≥ kMajorMinPx）的最小单位；没有达标者则整级隐藏
    for (int64_t unit : ladder) {
        if (double(unit) / 1000.0 * pxPerSecond >= double(kMajorMinPx)) {
            majorMs = unit;
            break;
        }
    }
    if (majorMs == 0) {
        return false;
    }

    // 次刻度：主刻度下一级里，约 4 等分（细分足够）且像素间距达标（≥ kMinorMinPx）的
    // 最大单位；达不到则次刻度级整体隐藏
    int majorIndex = ladder.indexOf(majorMs);
    for (int i = majorIndex - 1; i >= 0; --i) {
        const int64_t unit = ladder[i];
        if (unit * 4 > majorMs) {
            continue;   // 细分太少
        }
        if (double(unit) / 1000.0 * pxPerSecond < double(kMinorMinPx)) {
            continue;   // 太密（间距不足）
        }
        minorMs = unit;
        break;
    }
    return true;
}

QList<int64_t> TimelineWidget::ticksInRange(int64_t fromMs, int64_t toMs, int64_t unitMs) {
    QList<int64_t> result;
    if (unitMs <= 0 || toMs < fromMs) {
        return result;
    }

    // 对齐基准：≥1 分钟从本地零点起算（保证标签是本地整点/整分），
    // <1 分钟按 epoch 对齐（时区偏移都是整分钟，等价于本地整单位）
    int64_t base = fromMs - (fromMs % unitMs);
    if (unitMs >= 60000) {
        const QDateTime dt = QDateTime::fromMSecsSinceEpoch(fromMs);
        base = QDateTime(dt.date(), QTime(0, 0, 0)).toMSecsSinceEpoch();
    }

    int64_t k = (fromMs > base) ? ((fromMs - base + unitMs - 1) / unitMs) : 0;
    int64_t t = base + k * unitMs;
    while (t <= toMs) {
        result.append(t);
        t += unitMs;
    }
    return result;
}

QString TimelineWidget::tickLabel(int64_t timeMs, int64_t unitMs) {
    const QDateTime dt = QDateTime::fromMSecsSinceEpoch(timeMs);
    if (unitMs < 1000) {
        return dt.toString("HH:mm:ss.zzz");
    }
    if (unitMs < 60000) {
        return dt.toString("HH:mm:ss");
    }
    if (unitMs < 43200000LL) {   // < 12 小时
        return dt.toString("HH:mm");
    }
    if (unitMs < 86400000LL) {   // 12 小时级：整点标日期，其余标时刻
        return (dt.time() == QTime(0, 0, 0)) ? dt.toString("MM-dd") : dt.toString("HH:mm");
    }
    return dt.toString("MM-dd");
}

// ── 构造与基本状态 ─────────────────────────────────────────────────────

TimelineWidget::TimelineWidget(QWidget* parent)
    : QWidget(parent)
{
    setObjectName("TimelineWidget");
    setAttribute(Qt::WA_StyledBackground, true);
    setMinimumHeight(kWidgetHeight);
    setMaximumHeight(kWidgetHeight);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
    connect(ThemeManager::instance(), &ThemeManager::themeChanged, this,
            [this](ThemeManager::Theme) { update(); });
}

TimelineWidget::~TimelineWidget() {
}

void TimelineWidget::setTimeRange(int64_t startMs, int64_t endMs) {
    // 换了日期就是换了一批数据：回到「适配整段」；同一天翻页则保留当前跨度
    const QDate oldDay = QDateTime::fromMSecsSinceEpoch(m_startTimeMs).date();
    const QDate newDay = QDateTime::fromMSecsSinceEpoch(startMs).date();
    if (oldDay != newDay) {
        m_fitToRange = true;
    }
    m_startTimeMs = startMs;
    m_endTimeMs = endMs;
    // 「适配整段」模式下范围变化要跟着变；用户手动缩放过就保持当前跨度
    if (m_fitToRange) {
        m_visibleStartMs = startMs;
    }
    recomputeScale();
    clampViewport();
    update();
}

void TimelineWidget::setPeriods(const QList<RecordingPeriod>& periods) {
    m_periods = periods;
    // 按开始时间排序：绘制时用二分定位可见段，量只与视口内元素有关
    std::sort(m_periods.begin(), m_periods.end(),
              [](const RecordingPeriod& a, const RecordingPeriod& b) {
                  return a.startTimeMs < b.startTimeMs;
              });
    // 预录段可能与普通段重叠，终点不单调：额外维护前缀最大终点，
    // 向前回溯时据此判断「再往前不可能相交」
    m_periodPrefixMaxEnd.resize(m_periods.size());
    int64_t maxEnd = std::numeric_limits<int64_t>::min();
    for (int i = 0; i < m_periods.size(); ++i) {
        maxEnd = qMax(maxEnd, m_periods[i].endTimeMs);
        m_periodPrefixMaxEnd[i] = maxEnd;
    }
    update();
}

void TimelineWidget::setEvents(const QList<TimelineEvent>& events) {
    m_events = events;
    update();
}

void TimelineWidget::setBookmarks(const QList<TimelineEvent>& bookmarks) {
    m_bookmarks = bookmarks;
    update();
}

void TimelineWidget::addPlayhead(const QString& windowId, int colorIndex) {
    m_playheads[windowId] = {m_mainPlayheadMs, colorIndex};
    update();
}

QColor TimelineWidget::playheadColor(int colorIndex) {
    // 播放头是分类色：取主题里的 accent / success / warning / error / info
    static const char* const kColorKeys[] = {
        "accent", "success", "warning", "error", "info", "accent_hover"};
    const int count = int(sizeof(kColorKeys) / sizeof(kColorKeys[0]));
    const int index = ((colorIndex % count) + count) % count;
    return ThemeManager::instance()->getColor(QString::fromLatin1(kColorKeys[index]),
                                             QColor("#4a9eff"));
}

void TimelineWidget::removePlayhead(const QString& windowId) {
    m_playheads.remove(windowId);
    update();
}

void TimelineWidget::updatePlayhead(const QString& windowId, int64_t timeMs) {
    if (!m_playheads.contains(windowId)) {
        return;
    }
    m_playheads[windowId].timeMs = timeMs;
    // 只让选中窗口的那一路驱动视口跟随，避免多路不同步时来回跳
    if (windowId == m_highlightedPlayheadId) {
        followTime(timeMs);
    }
    update();
}

void TimelineWidget::clearAllPlayheads() {
    m_playheads.clear();
    m_highlightedPlayheadId.clear();
    update();
}

void TimelineWidget::setHighlightedPlayhead(const QString& windowId) {
    if (m_highlightedPlayheadId == windowId) {
        return;
    }
    m_highlightedPlayheadId = windowId;
    update();
}

void TimelineWidget::setCurrentTime(int64_t timeMs) {
    if (m_mainPlayheadMs == timeMs) {
        return;
    }
    m_mainPlayheadMs = timeMs;
    followTime(timeMs);
    update();
}

int64_t TimelineWidget::displayTimeMs() const {
    // 多路播放时各窗口播放头可能略有差异，取最靠后的那个作为当前读数
    int64_t latest = m_mainPlayheadMs;
    for (auto it = m_playheads.begin(); it != m_playheads.end(); ++it) {
        latest = qMax(latest, it.value().timeMs);
    }
    return latest;
}

void TimelineWidget::setVisibleSpan(int64_t spanMs) {
    const bool newFit = (spanMs <= 0);
    if (newFit == m_fitToRange && spanMs == m_visibleSpanMs) {
        return;
    }
    m_fitToRange = newFit;
    m_visibleSpanMs = spanMs;
    recomputeScale();   // 跨度小于 1s 时会被抬到 1s（最小可见跨度）
    clampViewport();
    update();
}

void TimelineWidget::setVisibleStart(int64_t timeMs) {
    m_visibleStartMs = timeMs;
    clampViewport();
    update();
}

// ── 视口与比例 ─────────────────────────────────────────────────────────

int TimelineWidget::contentWidth() const {
    return qMax(1, width() - margin * 2);
}

void TimelineWidget::recomputeScale() {
    int64_t span = m_fitToRange ? (m_endTimeMs - m_startTimeMs) : m_visibleSpanMs;
    if (span < kMinVisibleSpanMs) {
        span = kMinVisibleSpanMs;
    }
    m_visibleSpanMs = span;
    m_pxPerSecond = double(contentWidth()) * 1000.0 / double(span);
    m_zoom = m_pxPerSecond / double(kVirtualUnitsPerSecond);
}

void TimelineWidget::clampViewport() {
    if (m_endTimeMs <= m_startTimeMs) {
        m_visibleStartMs = m_startTimeMs;
        return;
    }
    int64_t maxStart = m_endTimeMs - m_visibleSpanMs;
    if (maxStart < m_startTimeMs) {
        maxStart = m_startTimeMs;
    }
    if (m_visibleStartMs < m_startTimeMs) {
        m_visibleStartMs = m_startTimeMs;
    }
    if (m_visibleStartMs > maxStart) {
        m_visibleStartMs = maxStart;
    }
}

void TimelineWidget::followTime(int64_t timeMs, bool force) {
    if (!force && followSuspendedNow()) {
        return;   // 用户刚手动平移过：先不打扰他
    }
    if (m_visibleSpanMs <= 0 || m_endTimeMs <= m_startTimeMs) {
        return;
    }
    const int64_t visibleEnd = m_visibleStartMs + m_visibleSpanMs;
    if (timeMs < m_visibleStartMs || timeMs > visibleEnd) {
        // 播放头移出视口：翻页跟随，落在左侧 10% 处
        m_visibleStartMs = timeMs - int64_t(double(m_visibleSpanMs) * 0.1);
        clampViewport();
    }
}

bool TimelineWidget::followSuspendedNow() const {
    if (m_manualNavUntilMs <= 0) {
        return false;
    }
    return QDateTime::currentMSecsSinceEpoch() < m_manualNavUntilMs;
}

void TimelineWidget::noteManualNavigation() {
    m_manualNavUntilMs = QDateTime::currentMSecsSinceEpoch() + kManualNavHoldMs;
}

void TimelineWidget::seekTo(int64_t timeMs) {
    // 显式定位：恢复跟随，并把视口带到目标位置
    m_manualNavUntilMs = 0;
    m_mainPlayheadMs = timeMs;
    // 定位作用于当前选中的窗口：它那一路播放头立刻跟过去
    if (!m_highlightedPlayheadId.isEmpty() && m_playheads.contains(m_highlightedPlayheadId)) {
        m_playheads[m_highlightedPlayheadId].timeMs = timeMs;
    }
    followTime(timeMs, true);
    update();
}

int TimelineWidget::timeToX(int64_t timeMs) const {
    if (m_visibleSpanMs <= 0) {
        return margin;
    }
    double ratio = double(timeMs - m_visibleStartMs) / double(m_visibleSpanMs);
    // 视口外的时间可能很远，夹一下避免 int 溢出
    ratio = qBound(-1.0, ratio, 2.0);
    return int(margin + ratio * contentWidth());
}

int64_t TimelineWidget::xToTime(int x) const {
    if (m_visibleSpanMs <= 0) {
        return m_startTimeMs;
    }
    const double ratio = double(x - margin) / double(contentWidth());
    const int64_t time = m_visibleStartMs + int64_t(ratio * double(m_visibleSpanMs));
    const int64_t visibleEnd = m_visibleStartMs + m_visibleSpanMs;
    return qBound(m_visibleStartMs, time, qMin(visibleEnd, m_endTimeMs));
}

int64_t TimelineWidget::msForPixels(double pixels) const {
    if (m_pxPerSecond <= 0.0) {
        return 0;
    }
    return int64_t(pixels * 1000.0 / m_pxPerSecond);
}

double TimelineWidget::pixelsForMs(int64_t ms) const {
    return double(ms) * m_pxPerSecond / 1000.0;
}

// ── 缩放 ──────────────────────────────────────────────────────────────

bool TimelineWidget::nextVisibleSpan(bool zoomIn, int64_t& spanMs) const {
    const int64_t range = m_endTimeMs - m_startTimeMs;
    if (range <= 0) {
        return false;
    }
    const QList<int64_t> ladder = zoomLadder();
    const int64_t current = m_visibleSpanMs > 0 ? m_visibleSpanMs : range;

    if (zoomIn) {
        int64_t candidate = 0;
        for (int64_t s : ladder) {
            if (s < current) {
                candidate = s;   // 升序遍历，最后一个满足的就是最大的
            }
        }
        if (candidate < kMinVisibleSpanMs) {
            return false;   // 已经在最细一级
        }
        spanMs = candidate;
        return true;
    }

    // 缩小：找比当前更大的最小档位；再往上就是「适配整段」
    int64_t candidate = range;
    for (int64_t s : ladder) {
        if (s > current && s < range) {
            candidate = s;
            break;
        }
    }
    spanMs = candidate;
    return true;
}

void TimelineWidget::zoomStep(bool zoomIn, int anchorX) {
    int64_t newSpan = 0;
    if (!nextVisibleSpan(zoomIn, newSpan)) {
        return;
    }

    // 以 anchorX 处的时间为锚点缩放：缩放前后该时间点仍在同一像素位置
    const int64_t anchorTime = xToTime(anchorX);
    m_fitToRange = (newSpan >= m_endTimeMs - m_startTimeMs);
    m_visibleSpanMs = newSpan;
    recomputeScale();
    const double ratio = double(anchorX - margin) / double(contentWidth());
    m_visibleStartMs = anchorTime - int64_t(ratio * double(newSpan));
    clampViewport();
    emit zoomChanged(m_zoom);
    update();
}

// ── 可见元素 ──────────────────────────────────────────────────────────

QList<TimelineWidget::MergedSegment> TimelineWidget::visibleSegments() const {
    QList<MergedSegment> merged;
    if (m_periods.isEmpty() || m_visibleSpanMs <= 0) {
        return merged;
    }
    const int64_t visibleStart = m_visibleStartMs;
    const int64_t visibleEnd = m_visibleStartMs + m_visibleSpanMs;

    // 录像段按 startTimeMs 排序：先二分定位第一个起点在可见区间之后的段，
    // 再向前回溯收集与窗口相交的段。
    // 预录段可能与普通段重叠，终点并不单调，不能对 endTimeMs 直接二分。
    const int end = int(std::lower_bound(m_periods.begin(), m_periods.end(), visibleEnd,
                                         [](const RecordingPeriod& p, int64_t v) {
                                             return p.startTimeMs < v;
                                         }) - m_periods.begin());

    QList<RecordingPeriod> hit;
    for (int i = end - 1; i >= 0; --i) {
        if (m_periodPrefixMaxEnd[i] <= visibleStart) {
            break;   // 再往前的段终点都不晚于窗口起点，不可能相交
        }
        const RecordingPeriod& period = m_periods[i];
        if (period.endTimeMs > visibleStart) {
            hit.prepend(period);
        }
    }

    for (const RecordingPeriod& period : hit) {
        if (!merged.isEmpty() && merged.last().type == period.segmentType &&
            merged.last().endMs >= period.startTimeMs) {
            // 同类且相邻/重叠：合并（只取边界，不填中间空档）
            if (period.endTimeMs > merged.last().endMs) {
                merged.last().endMs = period.endTimeMs;
            }
        } else {
            MergedSegment seg;
            seg.type = period.segmentType;
            seg.startMs = period.startTimeMs;
            seg.endMs = period.endTimeMs;
            merged.append(seg);
        }
    }
    return merged;
}

const RecordingPeriod* TimelineWidget::periodAt(int64_t timeMs) const {
    if (m_periods.isEmpty()) {
        return nullptr;
    }
    // 起点在 timeMs 之后的第一个
    const int idx = int(std::lower_bound(m_periods.begin(), m_periods.end(), timeMs,
                                         [](const RecordingPeriod& p, int64_t v) {
                                             return p.startTimeMs < v;
                                         }) - m_periods.begin());
    // 向前回溯找覆盖该时间的段（终点不单调，不能直接二分）
    for (int i = idx - 1; i >= 0; --i) {
        if (m_periodPrefixMaxEnd[i] <= timeMs) {
            break;
        }
        const RecordingPeriod& p = m_periods[i];
        if (p.startTimeMs <= timeMs && timeMs < p.endTimeMs) {
            return &p;   // 从后往前第一个命中的就是起点最晚的一个
        }
    }
    return nullptr;
}

// ── 绘制 ──────────────────────────────────────────────────────────────

void TimelineWidget::paintEvent(QPaintEvent* event) {
    Q_UNUSED(event)
    QWidget::paintEvent(event);
    QPainter p(this);
    p.setRenderHint(QPainter::Antialiasing, true);

    const ThemeManager* theme = ThemeManager::instance();
    const QColor bgColor = theme->getColor("tertiary_bg", QColor("#1e1e1a"));
    const QColor borderColor = theme->getColor("border", QColor("#2a2a2a"));
    const QColor textColor = theme->getColor("text_secondary", QColor("#888888"));
    const QColor strongTextColor = theme->getColor("text_primary", QColor("#ffffff"));
    const QColor accentColor = theme->getColor("accent", QColor("#4a9eff"));

    p.fillRect(rect().adjusted(1, 1, -1, -1), bgColor);

    const int contentLeft = margin;
    const int contentRight = width() - margin;
    const int contentW = qMax(1, contentRight - contentLeft);

    const bool hasRange = m_endTimeMs > m_startTimeMs;

    // ── 顶部一行：左侧加载范围，右侧播放头实时读数 ──
    QFont headerFont = p.font();
    headerFont.setPixelSize(11);
    p.setFont(headerFont);
    p.setPen(textColor);
    const QFontMetrics headerFm(headerFont);

    // 先算右侧读数的宽度，左侧范围文字要给它让位
    int playheadTextW = 0;
    QString playheadText;
    const int64_t displayTime = displayTimeMs();
    if (displayTime > 0) {
        int64_t majorUnit = 0;
        int64_t minorUnit = 0;
        if (chooseTickUnits(m_pxPerSecond, majorUnit, minorUnit) && majorUnit < 1000) {
            playheadText = QDateTime::fromMSecsSinceEpoch(displayTime).toString("HH:mm:ss.zzz");
        } else {
            playheadText = QDateTime::fromMSecsSinceEpoch(displayTime).toString("HH:mm:ss");
        }
        playheadTextW = headerFm.horizontalAdvance(playheadText);
    }

    QString rangeText;
    if (hasRange) {
        const QDateTime startDt = QDateTime::fromMSecsSinceEpoch(m_startTimeMs);
        const QDateTime endDt = QDateTime::fromMSecsSinceEpoch(m_endTimeMs);
        rangeText = QString("%1 %2 — %3 %4")
                        .arg(startDt.toString("MM-dd"))
                        .arg(startDt.toString("HH:mm"))
                        .arg(endDt.toString("MM-dd"))
                        .arg(endDt.toString("HH:mm"));
    } else {
        rangeText = tr("无录像");
    }

    const int rangeTextW = headerFm.horizontalAdvance(rangeText);
    if (rangeTextW + playheadTextW + 12 <= contentW) {
        p.drawText(QPoint(contentLeft, kHeaderBaseline), rangeText);
    }

    if (playheadTextW > 0) {
        p.setPen(strongTextColor);
        p.drawText(QPoint(contentRight - playheadTextW, kHeaderBaseline), playheadText);
    }

    int64_t majorUnit = 0;
    int64_t minorUnit = 0;
    const bool hasMajor = hasRange && chooseTickUnits(m_pxPerSecond, majorUnit, minorUnit);
    // ── 录像段条 ──
    const QRect barRect(contentLeft, kBarTop, contentW, kBarHeight);
    p.fillRect(barRect, borderColor);

    if (hasRange) {
        p.save();
        p.setClipRect(barRect);
        const QList<MergedSegment> segments = visibleSegments();
        for (const MergedSegment& seg : segments) {
            int x1 = timeToX(seg.startMs);
            int x2 = timeToX(seg.endMs);
            if (x2 < contentLeft) {
                continue;
            }
            if (x1 > contentRight) {
                break;
            }
            x1 = qMax(x1, contentLeft);
            x2 = qMin(x2, contentRight);
            if (x2 - x1 < 1) {
                x2 = x1 + 1;
            }
            const QRectF segRect(x1, kBarTop + 1, x2 - x1, kBarHeight - 2);
            p.setPen(Qt::NoPen);
            p.setBrush(segmentColor(theme, seg.type));
            p.drawRoundedRect(segRect, 3.0, 3.0);
        }
        p.restore();

        // ── 书签标记：画在录像段顶部 ──
        // 颜色按当前主题取（书签用 warning），而不是用事件自带的 color，
        // 这样运行中切换主题，标记颜色立刻跟着变。
        const QColor bookmarkColor = theme->getColor("warning", QColor("#ffd166"));
        p.setPen(Qt::NoPen);
        for (const TimelineEvent& e : m_bookmarks) {
            if (e.timeMs < m_visibleStartMs || e.timeMs > m_visibleStartMs + m_visibleSpanMs) {
                continue;
            }
            const int x = timeToX(e.timeMs);
            p.setBrush(bookmarkColor);
            p.drawRect(QRect(x, kBarTop, 3, 5));
        }

        // ── 事件标记（报警等）：画在录像段底部 ──
        const QColor eventColor = theme->getColor("error", QColor("#ff6b6b"));
        for (const TimelineEvent& e : m_events) {
            if (e.timeMs < m_visibleStartMs || e.timeMs > m_visibleStartMs + m_visibleSpanMs) {
                continue;
            }
            const int x = timeToX(e.timeMs);
            p.setPen(QPen(eventColor, 1));
            p.setBrush(eventColor);
            p.drawRect(QRect(x - 2, kBarTop + kBarHeight - 5, 5, 5));
        }
    }

    // ── 刻度 ──
    if (hasMajor) {
        const int64_t visibleStart = m_visibleStartMs;
        const int64_t visibleEnd = m_visibleStartMs + m_visibleSpanMs;

        // 次刻度（不标数字）
        if (minorUnit > 0) {
            const QList<int64_t> minorTicks = ticksInRange(visibleStart, visibleEnd, minorUnit);
            p.setPen(QPen(textColor, 1));
            for (int64_t t : minorTicks) {
                const int x = timeToX(t);
                p.drawLine(x, kMinorTickTop, x, kMinorTickTop + kMinorTickHeight);
            }
        }

        // 主刻度（带标签）
        const QList<int64_t> majorTicks = ticksInRange(visibleStart, visibleEnd, majorUnit);
        QFont labelFont = p.font();
        labelFont.setPixelSize(10);
        p.setFont(labelFont);
        const QFontMetrics fm(labelFont);
        p.setPen(QPen(strongTextColor, 1));
        int lastLabelRight = contentLeft - 1000;
        for (int64_t t : majorTicks) {
            const int x = timeToX(t);
            p.drawLine(x, kMajorTickTop, x, kMajorTickTop + kMajorTickHeight);

            const QString label = tickLabel(t, majorUnit);
            const int labelW = fm.horizontalAdvance(label);
            int labelX = x - labelW / 2;
            labelX = qBound(contentLeft, labelX, contentRight - labelW);
            if (labelX < lastLabelRight + 4) {
                continue;   // 与上一个标签重叠，跳过
            }
            lastLabelRight = labelX + labelW;
            p.drawText(QPoint(labelX, kLabelBaseline), label);
        }
    }

    // ── 窗口播放头（每路窗口一个，多实例）──
    for (auto it = m_playheads.begin(); it != m_playheads.end(); ++it) {
        const Playhead& ph = it.value();
        if (ph.timeMs < m_visibleStartMs || ph.timeMs > m_visibleStartMs + m_visibleSpanMs) {
            continue;   // 视口外：只保留内存状态，不绘制
        }
        const int x = timeToX(ph.timeMs);
        const bool highlighted = (it.key() == m_highlightedPlayheadId);
        const QColor color = playheadColor(ph.colorIndex);

        if (highlighted) {
            // 高亮：加一圈半透明光晕，一眼看出是哪个窗口
            QColor halo = color;
            halo.setAlpha(70);
            p.setPen(QPen(halo, 8));
            p.drawLine(x, kBarTop, x, kBarTop + kBarHeight);
        }

        p.setPen(QPen(color, highlighted ? 3 : 2));
        p.drawLine(x, kBarTop, x, kBarTop + kBarHeight);

        // 手柄压在录像段内部，不与顶部一行文字打架
        p.setPen(Qt::NoPen);
        p.setBrush(color);
        const int handle = highlighted ? 8 : 6;
        p.drawRect(QRect(x - handle / 2, kBarTop + 1, handle, handle));
    }

    // 播放头不分主次：每路窗口一个，选中窗口的那一路用高亮样式区分
}

// ── 交互 ──────────────────────────────────────────────────────────────

void TimelineWidget::mousePressEvent(QMouseEvent* event) {
    if (event->button() == Qt::MiddleButton ||
        (event->button() == Qt::LeftButton && event->modifiers() & Qt::ShiftModifier)) {
        m_isDragging = true;
        m_dragStartX = event->x();
        m_dragStartTime = m_visibleStartMs;
        noteManualNavigation();
        grabMouse();   // 在控件外松手也要收到事件，避免拖拽状态卡住
        return;
    }

    if (event->button() == Qt::LeftButton) {
        // 左键按在播放头上：直接进入拖拽，不用先双击
        for (auto it = m_playheads.begin(); it != m_playheads.end(); ++it) {
            if (qAbs(event->x() - timeToX(it.value().timeMs)) <= 8) {
                m_draggingPlayheadId = it.key();
                m_isDraggingPlayhead = true;
                return;
            }
        }
        // 否则先记下按下位置：左右拖动就平移视口（定位用双击）
        m_pressPos = event->pos();
        m_dragStartTime = m_visibleStartMs;
        m_pendingDrag = true;
    }
}

void TimelineWidget::mouseDoubleClickEvent(QMouseEvent* event) {
    if (event->button() != Qt::LeftButton) {
        return;
    }
    for (auto it = m_playheads.begin(); it != m_playheads.end(); ++it) {
        if (qAbs(event->x() - timeToX(it.value().timeMs)) < 15) {
            return;   // 播放头拖拽由 press/move 处理
        }
    }

    const int64_t clickedTime = xToTime(event->x());
    seekTo(clickedTime);   // 移动当前选中窗口的播放头
    emit timeSelected(clickedTime);

    if (!m_highlightedPlayheadId.isEmpty() && m_playheads.contains(m_highlightedPlayheadId)) {
        emit selectedWindowPlayheadMoved(m_highlightedPlayheadId, clickedTime);
    }

    if (const RecordingPeriod* period = periodAt(clickedTime)) {
        emit periodClicked(*period);
    }
    update();
}

void TimelineWidget::mouseMoveEvent(QMouseEvent* event) {
    // 左键按下后拖动超过阈值：切换成「左右平移视口」
    if (m_pendingDrag && (event->pos() - m_pressPos).manhattanLength() > 3) {
        m_pendingDrag = false;
        m_isDragging = true;
        m_dragStartX = m_pressPos.x();
        noteManualNavigation();
        setCursor(Qt::ClosedHandCursor);
        grabMouse();
    }

    if (m_isDragging) {
        // 水平拖动：像素位移换算成时间位移（往右拖 = 看更早的时间）
        const double deltaPx = double(event->x()) - m_dragStartX;
        const int64_t deltaMs = msForPixels(deltaPx);
        m_visibleStartMs = m_dragStartTime - deltaMs;
        clampViewport();
        update();
        return;
    }

    if (m_isDraggingPlayhead) {
        const int64_t time = xToTime(event->x());
        if (m_playheads.contains(m_draggingPlayheadId)) {
            m_playheads[m_draggingPlayheadId].timeMs = time;
            emit playheadMoved(m_draggingPlayheadId, time);
            update();
        }
        return;
    }

    if (const RecordingPeriod* period = periodAt(xToTime(event->x()))) {
        const QDateTime start = QDateTime::fromMSecsSinceEpoch(period->startTimeMs);
        const QDateTime end = QDateTime::fromMSecsSinceEpoch(period->endTimeMs);
        const QString tip = QString("%1 - %2\n类型: %3")
            .arg(start.toString("HH:mm:ss"))
            .arg(end.toString("HH:mm:ss"))
            .arg(period->segmentType == SegmentType::Alarm ? "报警"
                 : (period->segmentType == SegmentType::PreBuffer ? "预录" : "普通"));
        setToolTip(tip);
        return;
    }

    setToolTip(QString());
}

void TimelineWidget::resizeEvent(QResizeEvent* event) {
    QWidget::resizeEvent(event);
    // 宽度变了：比例要重算，视口要重新夹紧
    recomputeScale();
    clampViewport();
    update();
}

void TimelineWidget::mouseReleaseEvent(QMouseEvent* event) {
    if (event->button() == Qt::MiddleButton ||
        (event->button() == Qt::LeftButton && event->modifiers() & Qt::ShiftModifier)) {
        m_isDragging = false;
        releaseMouse();
    }

    if (event->button() == Qt::LeftButton) {
        if (m_isDragging) {
            // 左键拖动平移结束
            m_isDragging = false;
            unsetCursor();
            releaseMouse();
        } else if (m_isDraggingPlayhead) {
            const QString releasedId = m_draggingPlayheadId;
            m_isDraggingPlayhead = false;
            m_draggingPlayheadId.clear();
            if (!releasedId.isEmpty() && m_playheads.contains(releasedId)) {
                emit playheadReleased(releasedId, m_playheads[releasedId].timeMs);
            }
        }
        // 单击不定位：定位统一用双击，避免拖动时误跳
        m_pendingDrag = false;
    }
}

void TimelineWidget::wheelEvent(QWheelEvent* event) {
    if (event->modifiers() & Qt::ShiftModifier) {
        // Shift + 滚轮：水平滚动（一格滚轮 = kWheelScrollPixels 像素）
        const int notches = event->angleDelta().y() / 120;
        if (notches == 0) {
            event->accept();
            return;
        }
        const int64_t deltaMs = msForPixels(-double(notches) * kWheelScrollPixels);
        m_visibleStartMs += deltaMs;
        clampViewport();
        noteManualNavigation();   // 用户在看别处，短时间内不自动跟随
        update();
        event->accept();
        return;
    }

    const int anchorX = int(event->position().x());
    zoomStep(event->angleDelta().y() > 0, anchorX);
    event->accept();
}

}
