#pragma once

#include <QWidget>
#include <QMap>
#include <QSet>
#include <QString>
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
    bool bindChannelToCell(const QString& channelKey, const QString& cellId);
    void unbindCell(const QString& cellId);
    QString channelOfCell(const QString& cellId) const;
    QStringList cellsOfChannel(const QString& channelKey) const;
    /// 通道的播放组（不存在则创建）
    PlaybackGroup* channelGroup(const QString& channelKey);

    // ── 通道的片段装载与播放 ───────────────────────────────────────
    void applyChannelClips(const QString& channelKey, const QList<Clip>& clips,
                           int64_t rangeStartMs, int64_t rangeEndMs);
    void seekChannel(const QString& channelKey, int64_t timeMs);
    void playChannel(const QString& channelKey, int64_t timeMs);

    PlaybackCell* getCell(const QString& cellId) const;
    QList<QString> getCellIds() const { return m_cells.keys(); }
    PlaybackController* controller() const { return m_controller; }
    /// 当前目标窗口（点击录像文件会播放到这里），空表示未选中
    QString targetCellId() const { return m_selectedCellId; }

public slots:
    void play();
    void pause();
    void stop();
    void setSpeed(double speed);

signals:
    void cellClicked(int cellId, const QString& deviceId);
    /// 播放进度（绝对时间，毫秒）
    void playbackProgress(int64_t timeMs);
    /// 某个通道的播放进度（绝对时间，毫秒）
    void channelPlaybackProgress(const QString& channelKey, int64_t timeMs);
    /// 目标窗口变化（用户点击了某个回放窗口）
    void targetCellChanged(const QString& cellId);
    /// 窗口绑定的通道发生变化
    void cellChannelChanged(const QString& cellId, const QString& channelKey);
    /// 录像文件被拖到某个播放窗口（决定那个窗口播哪个通道）
    void recordingDropped(const QString& channelKey, int64_t startTimeMs, const QString& cellId);

private slots:
    void onLayoutsReady();
    void onLayoutSelected(int index);
    void onCellClicked();
    void onCellProgressChanged(const QString& cellId, int64_t timeMs);
    /// 有播放器被拖到另一个窗口上：两个播放器交换位置
    void onPlayerDropped(PlaybackCell* target, const QString& sourceUid);

protected:
    void resizeEvent(QResizeEvent* event) override;
    void dragEnterEvent(QDragEnterEvent* event) override;
    void dragMoveEvent(QDragMoveEvent* event) override;
    void dropEvent(QDropEvent* event) override;

private:
    void calculateLayout(const PlayerLayout& layout);
    QString selectTargetCell() const;
    void setupTimelineConnections();
    PlaybackCell* findCellByUid(const QString& uid) const;

    QWidget* m_videoArea;
    ReplayNavBar* m_navBar;
    QStackedWidget* m_timelineStack;
    QList<PlayerLayout> m_layouts;
    PlayerLayout m_currentLayout;
    QMap<QString, PlaybackCell*> m_cells;
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
