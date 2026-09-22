#pragma once

#include "interfaces/IPlugin.h"
#include <QObject>
#include <QString>
#include <QDateTime>

namespace rustcam {

class PlaybackPlugin : public QObject, public IPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID "com.rustcam.IPlugin/1.0" FILE "plugin.json")
    Q_INTERFACES(rustcam::IPlugin)
public:
    QString id() const override { return "playback"; }
    QString displayName() const override { return tr("录像回放"); }
    QString version() const override { return "1.0.0"; }
    QIcon icon() const override { return QIcon(":/icons/playback.svg"); }

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
    void onSeek(const QDateTime& time);

private:
    void loadRecordings(const QString& deviceId, const QDateTime& start, const QDateTime& end);

    QWidget* m_workspace;
    QString m_currentDeviceId;
    QDateTime m_playbackTime;
    bool m_isPlaying;
    double m_playbackSpeed;
};

}
