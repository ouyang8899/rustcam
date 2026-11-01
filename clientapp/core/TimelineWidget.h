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
    /// 事件自身的颜色（供列表等其它界面使用）。
    /// 时间轴上的报警/书签标记统一按当前主题取色，切主题时能立刻生效。
    QColor color = QColor("#ff6b6b");
    QString label;
};

/// 录像回放页的视频区下方那条时间轴（同时充当进度条）
///
/// 内部用「视口」模型：加载范围 [m_startTimeMs, m_endTimeMs] 是数据全集，
/// 视口 [m_visibleStartMs, m_visibleStartMs + m_visibleSpanMs] 是当前看到的部分。
/// 缩放改变的是可见跨度，不是数据范围，因此缩放/滚动/改窗口大小都只是
/// 改几个内存参数，绘制时按视口裁剪即可。
class RUSTCAM_CORE_API TimelineWidget : public QWidget {
    Q_OBJECT
public:
    struct Playhead {
        int64_t timeMs;
        /// 配色索引：实际颜色在绘制时按当前主题取，主题切换自动跟着变
        int colorIndex = 0;
    };

    /// 刻度阶梯（毫秒，升序）。最细 100ms（十分之一秒）。
    static QList<int64_t> tickLadder();
    /// 缩放阶梯：可见跨度（毫秒，升序）。最细 1s，放到最大时正好 10 根 100ms 刻度。
    static QList<int64_t> zoomLadder();

    /// 按像素阈值挑选主/次刻度单位。
    /// 主刻度：阶梯中「像素间距 ≥ kMajorMinPx」的最小单位；没有则主刻度级整体隐藏。
    /// 次刻度：主刻度下一级中，约 4 等分且像素间距 ≥ kMinorMinPx 的单位；
    ///         达不到则次刻度级整体隐藏。
    /// @param pxPerSecond 1 秒占多少真实像素
    static bool chooseTickUnits(double pxPerSecond, int64_t& majorMs, int64_t& minorMs);

    explicit TimelineWidget(QWidget* parent = nullptr);
    ~TimelineWidget() override;

    /// 设置加载范围（数据全集）。
    /// 「适配整段」模式下视口跟着范围变；用户手动缩放过则保持当前跨度。
    void setTimeRange(int64_t startMs, int64_t endMs);
    int64_t startTimeMs() const { return m_startTimeMs; }
    int64_t endTimeMs() const { return m_endTimeMs; }

    void setPeriods(const QList<RecordingPeriod>& periods);
    /// 事件标记（报警等），画在录像段下方
    void setEvents(const QList<TimelineEvent>& events);
    const QList<TimelineEvent>& events() const { return m_events; }

    /// 书签标记
    void setBookmarks(const QList<TimelineEvent>& bookmarks);
    const QList<TimelineEvent>& bookmarks() const { return m_bookmarks; }

    /// 可见窗口内的录像段（同类相邻段已合并）
    struct MergedSegment {
        SegmentType type;
        int64_t startMs;
        int64_t endMs;
    };
    QList<MergedSegment> visibleSegments() const;
    /// 落在给定时间上的录像段（起点最晚的那一段）；没有则 nullptr
    const RecordingPeriod* periodAt(int64_t timeMs) const;

    /// 主刻度时间点（本地时间整单位对齐）
    static QList<int64_t> ticksInRange(int64_t fromMs, int64_t toMs, int64_t unitMs);
    /// 刻度标签格式，随主刻度单位变化
    static QString tickLabel(int64_t timeMs, int64_t unitMs);

    void addPlayhead(const QString& windowId, int colorIndex = 0);
    void removePlayhead(const QString& windowId);
    void updatePlayhead(const QString& windowId, int64_t timeMs);
    void clearAllPlayheads();
    const QMap<QString, Playhead>& playheads() const { return m_playheads; }

    /// 播放头配色（取自当前主题的分类色），索引超出时循环取用
    static QColor playheadColor(int colorIndex);
    /// 可用的播放头配色数量
    static constexpr int playheadColorCount = 6;

    /// 高亮某个播放头（只是高亮：绘制上加粗加光晕，操作方式与其他播放头一致）
    void setHighlightedPlayhead(const QString& windowId);
    QString highlightedPlayheadId() const { return m_highlightedPlayheadId; }

    /// 主播放头（通道级）：一定会被绘制，播放时随进度前进
    void setCurrentTime(int64_t timeMs);
    int64_t currentTime() const { return m_mainPlayheadMs; }
    /// 顶部读数用的时间：各窗口播放头里最靠后的那个（多路播放时取领先的一路）
    int64_t displayTimeMs() const;

    /// 定位到某个时间：视口跟到目标位置，并恢复「播放头跟随」
    void seekTo(int64_t timeMs);

    /// 像素 -> 毫秒（按当前比例）
    int64_t msForPixels(double pixels) const;
    /// 毫秒 -> 像素（按当前比例）
    double pixelsForMs(int64_t ms) const;

    /// 可见跨度（毫秒）；0 表示「适配整段加载范围」
    void setVisibleSpan(int64_t spanMs);
    int64_t visibleSpanMs() const { return m_visibleSpanMs; }
    /// 视口左缘对应的绝对时间
    void setVisibleStart(int64_t timeMs);
    int64_t visibleStartMs() const { return m_visibleStartMs; }

    /// 缩放级别：真实像素 / 虚拟单位
    double zoomLevel() const { return m_zoom; }
    /// 1 秒占多少真实像素
    double pixelsPerSecond() const { return m_pxPerSecond; }
    /// 「虚拟标准」：1 秒 = kVirtualUnitsPerSecond 个虚拟单位
    static constexpr int kVirtualUnitsPerSecond = 10;

    /// 绝对时间 -> 控件横坐标
    int timeToX(int64_t timeMs) const;
    /// 控件横坐标 -> 绝对时间（夹在视口内）
    int64_t xToTime(int x) const;

    /// 缩放一档（zoomIn=true 放大，false 缩小），以 anchorX 处的时间为锚点
    void zoomStep(bool zoomIn, int anchorX);

    /// 用户手动平移/缩放过：暂停「播放头跟随」，显式定位后恢复
    bool followSuspended() const { return followSuspendedNow(); }
    /// 记一次用户手动平移：短时间内不自动跟随
    void noteManualNavigation();

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
    /// 可见跨度在阶梯上前进/后退一档；返回 false 表示已经在极限
    bool nextVisibleSpan(bool zoomIn, int64_t& spanMs) const;
    /// 依据当前宽度与可见跨度重算比例
    void recomputeScale();
    /// 可用内容宽度（去掉左右边距）
    int contentWidth() const;
    /// 把视口夹回合法范围
    void clampViewport();
    /// 播放头移出视口时翻页跟随；force=true 时无视「用户手动操作过」的暂停
    void followTime(int64_t timeMs, bool force = false);
    /// 用户手动平移是否仍在「暂停跟随」的有效期内
    bool followSuspendedNow() const;
private:
    /// 主刻度最小像素间距：低于该值整级不画
    static constexpr int kMajorMinPx = 60;
    /// 次刻度最小像素间距：低于该值整级不画
    static constexpr int kMinorMinPx = 10;
    /// 最小可见跨度 1s（此时 100ms 为主刻度，正好 10 根）
    static constexpr int64_t kMinVisibleSpanMs = 1000;
    /// 控件左右边距
    static const int margin = 4;
    /// 控件高度
    static constexpr int kWidgetHeight = 46;
    /// Shift+滚轮一格滚动的像素数
    static constexpr int kWheelScrollPixels = 100;
    /// 用户手动平移后暂停自动跟随的时长（毫秒）
    static constexpr qint64 kManualNavHoldMs = 2000;

    int64_t m_startTimeMs = 0;
    int64_t m_endTimeMs = 0;
    int64_t m_mainPlayheadMs = 0;

    /// 视口
    int64_t m_visibleStartMs = 0;
    int64_t m_visibleSpanMs = 0;
    /// 是否处于「适配整段」模式（范围变化时自动跟随，用户手动缩放后为 false）
    bool m_fitToRange = true;
    double m_pxPerSecond = 0.0;
    double m_zoom = 1.0;

    QList<RecordingPeriod> m_periods;
    /// m_periods 的前缀最大终点：用于向前回溯定位可见段
    /// （预录段与普通段可能重叠，终点不单调，不能对 endTimeMs 直接二分）
    QVector<int64_t> m_periodPrefixMaxEnd;
    QList<TimelineEvent> m_events;
    QList<TimelineEvent> m_bookmarks;

    QMap<QString, Playhead> m_playheads;
    /// 高亮的播放头（当前选中的播放窗口对应的那一路）
    QString m_highlightedPlayheadId;

    bool m_isDragging = false;
    double m_dragStartX = 0;
    int64_t m_dragStartTime = 0;
    /// 用户手动平移的截止时间（epoch 毫秒）：这段时间内不自动跟随
    qint64 m_manualNavUntilMs = 0;

    QString m_draggingPlayheadId;
    bool m_isDraggingPlayhead = false;
    /// 左键按下位置：拖动超过阈值转为平移视口（定位用双击）
    QPoint m_pressPos;
    bool m_pendingDrag = false;
};

}
