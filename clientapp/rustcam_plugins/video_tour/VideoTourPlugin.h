#pragma once

#include "interfaces/IPlugin.h"

#include <QObject>
#include <QWidget>

#include "core/EventBus.h"

namespace rustcam {

class TourApi;
class TourRunWidget;

/// 视频轮询插件：按方案把设备通道分批轮询到网格中播放。
class VideoTourPlugin : public IPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID "com.rustcam.IPlugin/1.0" FILE "plugin.json")
    Q_INTERFACES(rustcam::IPlugin)
public:
    VideoTourPlugin() = default;
    ~VideoTourPlugin() override = default;

    QString id() const override { return QStringLiteral("video_tour"); }
    QString displayName() const override { return tr("视频轮询"); }
    QString version() const override { return QStringLiteral("1.0.0"); }
    bool isWorkspace() const override { return true; }

    bool initialize(EventBus* bus, QObject* cfg) override;
    void shutdown() override;
    QWidget* createWorkspace(QWidget* parent) override;
    QWidget* createSidePanel(QWidget* parent) override;

private slots:
    void onLoggedIn(const QString& topic, EventDataPtr data);
    void onDataLoaded(bool success);

private:
    TourApi* m_api = nullptr;
    TourRunWidget* m_runWidget = nullptr;
    QString m_sessionToken;
};

} // namespace rustcam
