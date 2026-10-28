#pragma once

#include "interfaces/IPlugin.h"
#include <QString>
#include <QDateTime>
#include <QList>
#include <QMap>
#include <QPair>
#include <QSet>
#include <QtConcurrent>
#include <QFutureWatcher>
#include <QJsonObject>
#include <QJsonArray>
#include <QLabel>
#include <QDateTimeEdit>
#include "core/DeviceTreeNode.h"
#include "core/TimelineWidget.h"
#include "core/RecordingTypes.h"

namespace rustcam {

class PlaybackSidePanel;
class ReplayPlayerContainer;
class PlaybackChannel;

class PlaybackPlugin : public IPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID "com.rustcam.IPlugin/1.0" FILE "plugin.json")
    Q_INTERFACES(rustcam::IPlugin)
public:
    QString id() const override { return "playback"; }
    QString displayName() const override { return tr("录像回放"); }
    QString version() const override { return "1.0.0"; }

    bool initialize(EventBus* bus, QObject* cfg) override;
    void shutdown() override;
    QWidget* createWorkspace(QWidget* parent) override;
    QWidget* createSidePanel(QWidget* parent) override { Q_UNUSED(parent); return nullptr; }
    bool isWorkspace() const override { return true; }

private slots:
    void onPlaybackRequested(const QString& topic, const QVariantMap& payload);
    void onPlay();
    void onPause();
    void onStop();
    void onSpeedTextChanged(const QString& text);
    void onTimeEditChanged(const QDateTime& dt);
    void onSeek(int64_t timeMs);
    void onCellClicked(const QString& layoutItemId, const QString& deviceId);
    void onDeviceSelected(const QString& deviceTag, const QString& channelTag, const QString& deviceName);

    void onChannelRequested(const QString& deviceTag, const QString& channelTag,
                            const QString& displayName);
    void onActiveChannelChanged(const QString& key);
    void onChannelClosed(const QString& key);
    void onChannelFilesLoaded(const QString& key, const QList<RecordingFile>& files);
    void onChannelRangeChanged(const QString& key, int64_t startMs, int64_t endMs);
    void onChannelAlarmsLoaded(const QString& key, const QList<TimelineEvent>& events);
    void onChannelBookmarksChanged(const QString& key, const QList<TimelineEvent>& bookmarks);
    void onChannelPlayFileRequested(const QString& key, const RecordingFile& file);

    void onChannelPlaybackProgress(const QString& channelKey, int64_t timeMs);
    void onTargetCellChanged(const QString& layoutItemId);
    void onRecordingDropped(const QString& layoutItemId, const QString& channelKey, int64_t startTimeMs);
    void onChannelDropped(const QString& layoutItemId, const QString& deviceTag,
                          const QString& channelTag, const QString& displayName);
    void onUpdateTimer();

    void onExportClip();
    void onLoadDeviceTree();
    void onDataManagerLoaded();

private:
    /// 取得或创建通道（并把它的控件接到播放器上）
    PlaybackChannel* ensureChannel(const QString& deviceTag, const QString& channelTag,
                                   const QString& displayName);
    /// 把通道绑到窗口并激活（cellId 为空时用当前选中的窗口）
    void activateChannel(const QString& key, const QString& cellId);
    /// 用通道的录像文件装载并播放
    void playChannelFrom(const QString& key, int64_t startTimeMs);
    /// 把通道显示名称写到格子里的播放器（截图文件名用）
    void applyChannelNameToCell(PlaybackChannel* channel, const QString& cellId);
    void updateTimeEdit(int64_t timeMs);
    /// 当前激活通道（侧栏正在查看的通道）；工具栏动作只作用它
    QString activeChannelKey() const;

private:
    PlaybackSidePanel* m_sidePanel;
    ReplayPlayerContainer* m_container;
    QWidget* m_workspace = nullptr;
    QDateTime m_playbackTime;
    double m_playbackSpeed;
    QString m_currentDeviceId;
    /// 设备与通道标识（服务端按这两个字段精确匹配录像分段）
    QString m_currentDeviceTag;
    QString m_currentChannelTag;
    /// 当前目标回放窗口（双击录像文件播到这里）
    QString m_targetCellId;
    QLabel* m_targetLabel = nullptr;
    /// 工具栏的当前时间显示
    QLabel* m_currentTimeLabel = nullptr;
    /// 工具栏的时间输入框
    QDateTimeEdit* m_timeEdit = nullptr;
    QString m_playbackRequestToken;
    /// 通道 key -> 时间范围（该通道当前列出录像的总跨度）
    QMap<QString, QPair<int64_t, int64_t>> m_channelRanges;
    /// 拖拽通道后等录像加载完再自动播放
    QSet<QString> m_autoPlayChannels;
};

}
