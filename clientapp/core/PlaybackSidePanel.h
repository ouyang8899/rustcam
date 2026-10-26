#pragma once

#include <QWidget>
#include <QList>
#include <QVariantMap>
#include <QDate>
#include <QTabWidget>
#include "CoreExport.h"
#include "DeviceTree.h"
#include "RecordingFileListWidget.h"

class QComboBox;
class QStackedWidget;

namespace rustcam {

class PlaybackChannel;
class FileDownloader;

/// 录像回放侧栏 = 通道容器
/// 顶部下拉框切换「正在查看录像的通道」，侧栏内容按通道各自一份
class RUSTCAM_CORE_API PlaybackSidePanel : public QWidget {
    Q_OBJECT
public:
    explicit PlaybackSidePanel(QWidget* parent = nullptr);
    ~PlaybackSidePanel();

    DeviceTree* regionTree() const { return m_regionTree; }
    DeviceTree* groupTree() const { return m_groupTree; }
    DeviceTree* deviceTree() const { return m_deviceTree; }

    void setRegionTreeNodes(const QList<DeviceTreeNode::Ptr>& nodes);
    void setGroupTreeNodes(const QList<DeviceTreeNode::Ptr>& nodes);
    void setDeviceTreeNodes(const QList<DeviceTreeNode::Ptr>& nodes);

    // ── 通道管理 ───────────────────────────────────────────────────
    /// 登记一个通道（已存在则复用并激活）
    PlaybackChannel* registerChannel(const QString& deviceTag,
                                     const QString& channelTag,
                                     const QString& displayName);
    void unregisterChannel(const QString& key);
    bool hasChannel(const QString& key) const;
    PlaybackChannel* channel(const QString& key) const;
    QList<PlaybackChannel*> channels() const;

    /// 当前查看的通道
    PlaybackChannel* activeChannel() const;
    QString activeChannelKey() const;
    void setActiveChannel(const QString& key);

signals:
    /// 设备树里双击了一个通道（由外部决定绑到哪个播放窗口）
    void channelRequested(const QString& deviceTag, const QString& channelTag,
                          const QString& displayName);
    /// 当前查看的通道变化
    void activeChannelChanged(const QString& key);
    /// 用户请求移除当前通道
    void channelClosed(const QString& key);

private slots:
    void onDeviceItemDoubleClicked(QVariantMap meta);

private:
    void addChannelPage(const QString& key, PlaybackChannel* channel);

private:
    QTabWidget* m_tabWidget;
    DeviceTree* m_regionTree;
    DeviceTree* m_groupTree;
    DeviceTree* m_deviceTree;
    /// 通道切换下拉框
    QComboBox* m_channelCombo;
    /// 每个通道一页
    QStackedWidget* m_channelStack;
    /// 通道 key -> 通道
    QMap<QString, PlaybackChannel*> m_channels;
    /// 通道 key -> 侧栏页面
    QMap<QString, QWidget*> m_channelPages;
    /// 当前查看的通道 key
    QString m_activeChannelKey;
    /// 切换通道时避免下拉框回调递归
    bool m_switchingChannel = false;
};

}
