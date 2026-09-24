#pragma once

#include <QObject>
#include <QString>
#include <QIcon>
#include <QWidget>
#include <QFileInfo>
#include "InterfacesExport.h"

namespace rustcam {

class EventBus;
class ConfigManager;

#define RUSTCAM_PLUGIN_IID "com.rustcam.IPlugin/1.0"
#define RUSTCAM_PLUGIN_METADATA_IID "com.rustcam.ClientPlugin"

class INTERFACES_API IPlugin : public QObject {
    Q_OBJECT
public:
    virtual ~IPlugin() = default;

    virtual QString id() const = 0;
    virtual QString displayName() const = 0;
    virtual QString version() const = 0;
    QIcon icon() const { return m_icon; }

    void setPluginPath(const QString& path);

    virtual bool initialize(EventBus* bus, QObject* cfg) = 0;
    virtual void shutdown() = 0;

    virtual QWidget* createWorkspace(QWidget* parent) = 0;
    virtual QWidget* createSidePanel(QWidget* parent) = 0;

    virtual bool isWorkspace() const { return false; }
    QString shortcut() const { return m_shortcut; }

    virtual QStringList dependencies() const { return {}; }
    virtual QString apiVersion() const { return "1.0"; }

protected:
    IPlugin(QObject* parent = nullptr) : QObject(parent) {}

    QString m_pluginPath;
    mutable QIcon m_icon;
    QString m_shortcut;
};

}

Q_DECLARE_INTERFACE(rustcam::IPlugin, RUSTCAM_PLUGIN_IID)
