#pragma once

#include <QWidget>
#include <QVBoxLayout>
#include <QList>
#include <QDropEvent>
#include <QDragEnterEvent>
#include <QMimeData>
#include <QJsonDocument>
#include <QJsonObject>
#include "core/PlayerLayout.h"
#include "core/VideoWidgetManager.h"
#include "core/ServerApi.h"
#include "core/DataManager.h"
#include "core/VideoPlayerToolbar.h"

namespace rustcam {

class NavigationBar;
class VideoPlayer;

class VideoPlayerContainer : public QWidget {
    Q_OBJECT
public:
    explicit VideoPlayerContainer(QWidget* parent = nullptr);
    ~VideoPlayerContainer();

    void playStream(const QString& itemId, const QString& url, const QString& title);
    QString getSelectedItemId() const { return m_selectedItemId; }

public slots:
    void initialize();
    void onStreamRequested(const QString& topic);

private slots:
    void onLayoutsReady();
    void onLayoutSelected(int index);
    void onVideoPlayerClicked(const QString& itemId);
    void onChannelPlayLinksReceived(bool success, const QJsonObject& response, const QString& error, const QString& title, const QString& targetId);
    /// 某个播放器被点击（按 uid 定位到当前所在的格子）
    void onPlayerClicked(const QString& uid);
    /// 有播放器被拖到另一个位置：两个播放器对调（按 uid 定位两端）
    void onPlayerDropped(const QString& targetUid, const QString& sourceUid);
    /// 设备树节点被拖到某个播放器上（按 uid 定位到当前所在的格子）
    void onDeviceDropped(const QString& uid, const QVariantMap& meta);

private:
    void calculateVideoLayout(const PlayerLayout& layout);
    QString selectTargetItem();
    void handleDrop(const QVariantMap& meta, const QString& targetItemId = QString());
    /// 播放器所在的格子 id（按注册表反查）
    QString itemIdOfPlayer(VideoPlayer* player) const;
    /// 播放器所在的格子 id（按播放器 uid 反查，换位后依然正确）
    QString itemIdOfUid(const QString& uid) const;
    VideoPlayer* findPlayerByUid(const QString& uid) const;
    /// 把播放器摆进某个格子
    void layoutPlayerInWrapper(VideoPlayer* player, QWidget* wrapper);
    /// 选中态按格子重新应用
    void applySelectionStyles();

protected:
    void dragEnterEvent(QDragEnterEvent* event) override;
    void dragMoveEvent(QDragMoveEvent* event) override;
    void dropEvent(QDropEvent* event) override;
    void resizeEvent(QResizeEvent* event) override;

    QWidget* m_videoArea;
    NavigationBar* m_navBar;
    QList<PlayerLayout> m_layouts;
    PlayerLayout m_currentLayout;
    QString m_selectedItemId;
    QMap<QString, QString> m_playingUrls;
    QMap<QString, QWidget*> m_playerWrappers;
};

}
