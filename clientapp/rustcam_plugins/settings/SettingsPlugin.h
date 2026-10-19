#pragma once

#include "interfaces/IPlugin.h"
#include <QObject>
#include <QString>
#include <QWidget>
#include "core/SystemConfig.h"

namespace rustcam {

class SettingsWidget;

class SettingsPlugin : public IPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID "com.rustcam.IPlugin/1.0" FILE "plugin.json")
    Q_INTERFACES(rustcam::IPlugin)
public:
    QString id() const override { return "settings"; }
    QString displayName() const override { return tr("系统设置"); }
    QString version() const override { return "1.0.0"; }

    bool initialize(EventBus* bus, QObject* cfg) override;
    void shutdown() override;
    QWidget* createWorkspace(QWidget* parent) override;
    QWidget* createSidePanel(QWidget* parent) override { Q_UNUSED(parent) return nullptr; }
    bool isWorkspace() const override { return true; }

    void showSettingsDialog(QWidget* parent = nullptr);

private:
    SettingsWidget* m_settingsWidget = nullptr;
};

}
