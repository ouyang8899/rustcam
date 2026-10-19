#pragma once

#include "interfaces/IPlugin.h"
#include <QString>
#include <QDateTime>
#include <QList>
#include <QMap>
#include <QtConcurrent>
#include <QFutureWatcher>
#include <QJsonObject>
#include <QJsonArray>
#include "core/DeviceTreeNode.h"

namespace rustcam {

class PlaybackSidePanel;
class ReplayPlayerContainer;
class RecordingFile;

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
    void onSpeedChanged(double speed);
    void onSeek(int64_t timeMs);
    void onCellClicked(int cellId, const QString& deviceId);
    void onDeviceSelected(const QString& deviceId, const QString& deviceName);
    void onTimeSelected(int64_t timeMs);
    void onDateChanged(const QDate& date);
    void onFileDoubleClicked(const RecordingFile& file);
    void onLoadDeviceTree();
    void onDataManagerLoaded();

private:
    void loadRecordings(const QString& deviceId, const QDateTime& start, const QDateTime& end);

    PlaybackSidePanel* m_sidePanel;
    ReplayPlayerContainer* m_container;
    QDateTime m_playbackTime;
    double m_playbackSpeed;
    QString m_currentDeviceId;
    QString m_playbackRequestToken;
};

}
