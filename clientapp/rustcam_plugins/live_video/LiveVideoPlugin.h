#pragma once

#include "interfaces/IPlugin.h"
#include <QObject>
#include <QString>
#include <QMap>
#include <QWidget>
#include <QLabel>
#include <QtConcurrent>
#include <QFutureWatcher>
#include <QTabWidget>
#include <QJsonObject>
#include <QJsonArray>
#include <DeviceTree.h>
#include "VideoPlayerContainer.h"
#include "core/EventBus.h"
#include "core/DeviceTreeNode.h"
#include "core/DataManager.h"
#include "core/PTZControlPanel.h"

namespace rustcam {

class VideoCell : public QWidget {
    Q_OBJECT
public:
    VideoCell(const QString& cellId, QWidget* parent = nullptr);
    QString cellId() const { return m_cellId; }
    bool isPlaying() const { return m_playing; }
    void setTitle(const QString& title);
    void setPlaying(bool playing);
    void updateOsd(const QString& info);

 signals:
    void cellSelected(const QString& cellId);

private slots:
    void onCellClicked();

private:
    void setupUi();
    QString m_cellId;
    bool m_playing;
    QLabel* m_videoLabel;
    QWidget* m_overlayWidget;
    QLabel* m_osdLabel;
    QLabel* m_titleLabel;
};

class LiveVideoPlugin :  public IPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID "com.rustcam.IPlugin/1.0" FILE "plugin.json")
    Q_INTERFACES(rustcam::IPlugin)
public:
    QString id() const override { return "live_video"; }
    QString displayName() const override { return tr("实时视频"); }
    QString version() const override { return "1.0.0"; }

    bool initialize(EventBus* bus, QObject* cfg) override;
    void shutdown() override;
    QWidget* createWorkspace(QWidget* parent) override;
    QWidget* createSidePanel(QWidget* parent) override;

public slots:
    bool isWorkspace() const override { return true; }

    void playStream(const QString& cellId, const QString& url, const QString& title);
    void stopStream(const QString& cellId);
    void setLayout(int cells);

signals:
    void streamStarted(const QString& cellId);
    void streamStopped(const QString& cellId);

private slots:
    void onStreamRequested(const QString& topic, EventDataPtr);
    void onStreamStop(const QString& topic, EventDataPtr);
    void onAllDataLoaded();
    void onLoggedIn(const QString& topic, EventDataPtr);
    void onDataManagerLoaded();
    void onPtzCommand(const QString& command, const QVariant& value);

private:
    void updateLayout();
    VideoCell* getCell(const QString& cellId);
    void loadAllDeviceData();
    void buildDeviceTrees(const QJsonArray& regionsData, const QJsonArray& groupsData,
                         const QJsonArray& devicesData, const QJsonArray& channelsData);
    DeviceTreeNode::Ptr parseRegionNode(const QJsonObject& obj, const QMap<QString, QList<DeviceTreeNode::Ptr>>& regionDeviceMap);
    DeviceTreeNode::Ptr parseGroupNode(const QJsonObject& obj, const QMap<QString, QList<DeviceTreeNode::Ptr>>& groupDeviceMap);

    void onDeviceTreeItemClicked(const QVariantMap& meta);
    void onChannelPlayLinksReceived(bool success, const QJsonObject& response, const QString& error, const QString& title, const QString& itemId);

    QWidget* m_workspace;
    QWidget* m_sidePanel;
    QTabWidget* m_tabWidget;
    DeviceTree* m_regionTree;
    DeviceTree* m_groupTree;
    PTZControlPanel* m_ptzPanel;
    QMap<QString, VideoCell*> m_cells;
    int m_currentLayout;
    int m_maxCells;
    VideoPlayerContainer* m_videoContainer;
    QString m_sessionLoggedInToken;

    QFutureWatcher<QPair<bool, QJsonObject>>* m_allDataWatcher;
    QFuture<QPair<bool, QJsonObject>> m_regionsFuture;
    QFuture<QPair<bool, QJsonObject>> m_groupsFuture;
    QFuture<QPair<bool, QJsonObject>> m_devicesFuture;
    QFuture<QPair<bool, QJsonObject>> m_channelsFuture;
};

}
