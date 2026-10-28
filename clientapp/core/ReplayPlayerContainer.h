#pragma once

#include <QWidget>
#include <QMap>
#include <QSet>
#include <QString>
#include <QStringList>
#include <QStackedWidget>
#include <QJsonObject>
#include <QResizeEvent>
#include "CoreExport.h"
#include "PlayerLayout.h"
#include "DataManager.h"
#include "StreamSource.h"
#include "TimelineWidget.h"
#include "PlaybackController.h"
#include "PlaybackCell.h"
#include "PlaybackPlayerWidget.h"

namespace rustcam {

class PlaybackController;
class ReplayNavBar;

class RUSTCAM_CORE_API ReplayPlayerContainer : public QWidget {
    Q_OBJECT
public:
    explicit ReplayPlayerContainer(QWidget* parent = nullptr);
    ~ReplayPlayerContainer();

    void initialize();
    void setBaseTime(int64_t timeMs);

    // ── 通道时间轴（每个通道一条，显示在视频区下方）────────────────
    void attachChannelTimeline(const QString& channelKey, TimelineWidget* timeline);
    void removeChannelTimeline(const QString& channelKey);
    void setActiveChannelTimeline(const QString& channelKey);
    TimelineWidget* channelTimeline(const QString& channelKey) const;
    TimelineWidget* activeTimeline() const;

    // ── 播放窗口 ↔ 通道绑定 ────────────────────────────────────────
    /// 当前选中的播放窗口（布局项 id），没有选中时为空
    QString selectedCellId() const;
    void selectCell(const QString& cellId);
    bool bindChannelToCell(const QString& channelKey, const QString& cellId);
    void unbindCell(const QString& cellId);
    /// 悬浮工具栏关闭某个窗口：停止该格播放，必要时注销通道
    void closeCell(const QString& cellId);
    QString channelOfCell(const QString& cellId) const;
    QStringList cellsOfChannel(const QString& channelKey) const;
    /// 通道的播放组（不存在则创建）
    PlaybackGroup* channelGroup(const QString& channelKey);

    // ── 通道的片段装载与播放 ───────────────────────────────────────
    void applyChannelClips(const QString& channelKey, const QList<Clip>& clips,
                           int64_t rangeStartMs, int64_t rangeEndMs);
    /// 追加分片（加载更多时用，不打断正在进行的播放）
    void appendChannelClips(const QString& channelKey, const QList<Clip>& clips);
    /// 该通道是否正在播放
    bool isChannelPlaying(const QString& channelKey) const;
    void seekChannel(const QString& channelKey, int64_t timeMs);
    void playChannel(const QString& channelKey, int64_t timeMs);
    void pauseChannel(const QString& channelKey);
    void stopChannel(const QString& channelKey);
    void setChannelSpeed(const QString& channelKey, double speed);

    PlaybackCell* getCell(const QString& cellId) const;
    QList<QString> getCellIds() const { return m_cells.keys(); }
    PlaybackController* controller() const { return m_controller; }
    /// 当前目标窗口（点击录像文件会播放到这里），空表示未选中
    QString targetCellId() const { return m_selectedCellId; }

signals:
    void cellClicked(const QString& layoutItemId, const QString& deviceId);
    /// 某个通道的播放进度（绝对时间，毫秒）
    void channelPlaybackProgress(const QString& channelKey, int64_t timeMs);
    /// 关闭某个窗口后，该通道已没有任何窗口在播放，需要从回放页注销
    void channelCloseRequested(const QString& channelKey);
    /// 目标窗口变化（用户点击了某个回放窗口）
    void targetCellChanged(const QString& layoutItemId);
    /// 窗口绑定的通道发生变化
    void cellChannelChanged(const QString& layoutItemId, const QString& channelKey);
    /// 录像文件被拖到某个播放窗口（决定那个窗口播哪个通道）
    void recordingDropped(const QString& layoutItemId, const QString& channelKey, int64_t startTimeMs);
    /// 设备树通道被拖到某个播放窗口
    void channelDropped(const QString& layoutItemId, const QString& deviceTag,
                        const QString& channelTag, const QString& displayName);

private slots:
    void onLayoutsReady();
    void onLayoutSelected(int index);
    void onCellClicked(const QString& layoutItemId, const QString& deviceId);
    void onCellProgressChanged(const QString& layoutItemId, int64_t timeMs);
    /// 有播放器被拖到另一个窗口上：两个播放器交换位置
    void onPlayerDropped(const QString& targetLayoutItemId, const QString& sourceUid);
    void onRecordingDropped(const QString& layoutItemId, const QString& channelKey, int64_t startTimeMs);
    void onChannelDropped(const QString& layoutItemId, const QString& deviceTag,
                          const QString& channelTag, const QString& displayName);

protected:
    void resizeEvent(QResizeEvent* event) override;
    bool eventFilter(QObject* watched, QEvent* event) override;
    void dragEnterEvent(QDragEnterEvent* event) override;
    void dragMoveEvent(QDragMoveEvent* event) override;
    void dropEvent(QDropEvent* event) override;

private:
    void calculateLayout(const PlayerLayout& layout);
    QString selectTargetCell() const;
    void applySelectionStates();
    PlaybackCell* findCellByUid(const QString& uid) const;

    QWidget* m_videoArea;
    ReplayNavBar* m_navBar;
    QStackedWidget* m_timelineStack;
    QList<PlayerLayout> m_layouts;
    PlayerLayout m_currentLayout;
    QMap<QString, PlaybackCell*> m_cells;
    /// 上一次布局的 item id 顺序：resize 时按位置复用，避免 QMap 的 key 排序打乱
    QStringList m_cellOrder;
    /// 已经创建过播放头的窗口，避免布局重算时重复添加
    QSet<QString> m_playheads;
    QString m_selectedCellId;
    PlaybackController* m_controller;
    int m_totalRecords;
    int m_page;
    int m_limit;
    bool m_hasMore;
    /// 通道 key -> 时间轴控件
    QMap<QString, TimelineWidget*> m_channelTimelines;
    /// 通道 key -> 播放组
    QMap<QString, PlaybackGroup*> m_channelGroups;
    /// 当前显示的时间轴所属通道
    QString m_activeChannelKey;
};

}
