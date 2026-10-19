#include "SettingsPlugin.h"
#include "SettingsWidget.h"
#include "core/EventBus.h"
#include "common/Logger.h"

namespace rustcam {

bool SettingsPlugin::initialize(EventBus* bus, QObject* cfg) {
    Q_UNUSED(bus);
    Q_UNUSED(cfg);
    LOG_INFO("SettingsPlugin", "Settings plugin initialized");
    return true;
}

void SettingsPlugin::shutdown() {
    if (m_settingsWidget) {
        m_settingsWidget->deleteLater();
        m_settingsWidget = nullptr;
    }
    LOG_INFO("SettingsPlugin", "Settings plugin shutdown");
}

QWidget* SettingsPlugin::createWorkspace(QWidget* parent) {
    if (!m_settingsWidget) {
        m_settingsWidget = new SettingsWidget(parent);
    }
    return m_settingsWidget;
}

void SettingsPlugin::showSettingsDialog(QWidget* parent) {
    if (!m_settingsWidget) {
        m_settingsWidget = new SettingsWidget(parent);
    }
    m_settingsWidget->show();
    m_settingsWidget->raise();
    m_settingsWidget->activateWindow();
}

}
