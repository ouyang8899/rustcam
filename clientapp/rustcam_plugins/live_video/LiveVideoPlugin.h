#pragma once

#include "interfaces/IPlugin.h"
#include <QObject>
#include <QString>
#include <QMap>
#include <QVector>

namespace rustcam {

class VideoCell;

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
    QWidget* createSidePanel(QWidget* parent) override { Q_UNUSED(parent); return nullptr; }
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

private:
    void updateLayout();
    VideoCell* getCell(const QString& cellId);

    QWidget* m_workspace;
    QMap<QString, VideoCell*> m_cells;
    int m_currentLayout;
    int m_maxCells;
};

}
