#pragma once

#include "interfaces/IPlugin.h"
#include <QObject>
#include <QString>
#include <QMap>
#include <QVector>
#include <QWidget>
#include <QLabel>
#include <QtConcurrent>
#include <QFutureWatcher>
#include <QTabWidget>
#include <DeviceTree.h>
#include "VideoPlayerContainer.h"

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

class LiveVideoPlugin : public QObject, public IPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID "com.rustcam.IPlugin/1.0" FILE "plugin.json")
    Q_INTERFACES(rustcam::IPlugin)
public:
    QString id() const override { return "live_video"; }
    QString displayName() const override { return tr("实时视频"); }
    QString version() const override { return "1.0.0"; }
    QIcon icon() const override { return QIcon(":/icons/live_video.svg"); }

    bool initialize(EventBus* bus, QObject* cfg) override;
    void shutdown() override;
    QWidget* createWorkspace(QWidget* parent) override;
    QWidget* createSidePanel(QWidget* parent) override;

public slots:
    void onChannelClicked(int deviceId, int channelId, const QString& channelName);
    bool isWorkspace() const override { return true; }

    void playStream(const QString& cellId, const QString& url, const QString& title);
    void stopStream(const QString& cellId);
    void setLayout(int cells);

signals:
    void streamStarted(const QString& cellId);
    void streamStopped(const QString& cellId);

private slots:
    void onStreamRequested(const QString& topic, const QVariantMap& payload);
    void onStreamStop(const QString& topic, const QVariantMap& payload);
    void onRegionTreeFinished();
    void onGroupTreeFinished();

private:
    void updateLayout();
    VideoCell* getCell(const QString& cellId);
    void loadRegionTree();
    void loadGroupTree();
    void onDeviceTreeItemClicked(const QVariantMap& meta);

    QWidget* m_workspace;
    QWidget* m_sidePanel;
    QTabWidget* m_tabWidget;
    DeviceTree* m_regionTree;
    DeviceTree* m_groupTree;
    QMap<QString, VideoCell*> m_cells;
    QFutureWatcher<QPair<bool, QJsonObject>>* m_regionTreeWatcher;
    QFutureWatcher<QPair<bool, QJsonObject>>* m_groupTreeWatcher;
    int m_currentLayout;
    int m_maxCells;
    VideoPlayerContainer* m_tvWall;
};

}
