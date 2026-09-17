#pragma once

#include <QObject>
#include <QString>
#include <QMap>
#include <QStringList>
#include <QPluginLoader>

namespace rustcam {

class IPlugin;
class EventBus;
class ConfigManager;

class PluginManager : public QObject {
    Q_OBJECT
public:
    static PluginManager* instance();

    bool loadPlugins(const QString& pluginDir);
    void unloadPlugins();

    IPlugin* plugin(const QString& id) const;
    QList<IPlugin*> allPlugins() const;

    QStringList loadedPluginIds() const;
    bool isPluginLoaded(const QString& id) const;

    QString pluginVersion(const QString& id) const;

    bool meetsDependencies(const QString& id) const;
    QStringList missingDependencies(const QString& id) const;

    void setPluginEnabled(const QString& id, bool enabled);
    bool isPluginEnabled(const QString& id) const;

    QString lastError() const { return m_lastError; }

signals:
    void pluginLoaded(const QString& id, const QString& version);
    void pluginUnloaded(const QString& id);
    void pluginLoadFailed(const QString& id, const QString& reason);
    void allPluginsLoaded();

private:
    explicit PluginManager(QObject* parent = nullptr);
    ~PluginManager();
    Q_DISABLE_COPY(PluginManager)

    bool loadPlugin(const QString& filePath);
    bool initializePlugin(IPlugin* plugin);
    void shutdownPlugin(IPlugin* plugin);

    QMap<QString, QPluginLoader*> m_loaders;
    QMap<QString, IPlugin*> m_plugins;
    QMap<QString, bool> m_enabledPlugins;
    QString m_lastError;
};

}
