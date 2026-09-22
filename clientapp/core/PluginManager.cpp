#include "PluginManager.h"
#include "Logger.h"
#include "interfaces/IPlugin.h"
#include "EventBus.h"
#include "ConfigManager.h"
#include <QDir>

namespace rustcam {

PluginManager::PluginManager(QObject* parent)
    : QObject(parent)
{
}

PluginManager::~PluginManager()
{
    unloadPlugins();
}

PluginManager* PluginManager::instance()
{
    static PluginManager inst;
    return &inst;
}

bool PluginManager::loadPlugins(const QString& pluginDir)
{
    QDir dir(pluginDir);
    if (!dir.exists()) {
        LOG_WARNING("PluginManager", QString("Plugin directory does not exist: %1").arg(pluginDir));
        return false;
    }

    QStringList filters;
#if defined(Q_OS_WIN)
    filters << "*.dll";
#elif defined(Q_OS_MAC)
    filters << "*.dylib";
#else
    filters << "*.so";
#endif

    QStringList pluginFiles = dir.entryList(filters, QDir::Files);
    LOG_INFO("PluginManager", QString("Found %1 plugin files").arg(pluginFiles.size()));

    int loadedCount = 0;
    for (const QString& file : pluginFiles) {
        if (loadPlugin(dir.absoluteFilePath(file))) {
            loadedCount++;
        }
    }

    LOG_INFO("PluginManager", QString("Loaded %1 plugins").arg(loadedCount));
    emit allPluginsLoaded();

    return loadedCount > 0;
}

bool PluginManager::loadPlugin(const QString& filePath)
{
    QPluginLoader loader(filePath);
    QObject* obj = loader.instance();

    if (!obj) {
        m_lastError = loader.errorString();
        LOG_ERROR("PluginManager", QString("Failed to load plugin: %1 - %2")
                                    .arg(filePath, loader.errorString()));
        return false;
    }

    auto* plugin = qobject_cast<IPlugin*>(obj);
    if (!plugin) {
        m_lastError = "Plugin does not implement IPlugin interface";
        LOG_ERROR("PluginManager", QString("Invalid plugin: %1").arg(filePath));
        loader.unload();
        return false;
    }

    QString id = plugin->id();
    if (m_plugins.contains(id)) {
        LOG_WARNING("PluginManager", QString("Plugin already loaded: %1").arg(id));
        loader.unload();
        return false;
    }

    if (!initializePlugin(plugin)) {
        m_lastError = "Plugin initialization failed";
        LOG_ERROR("PluginManager", QString("Failed to initialize plugin: %1").arg(id));
        loader.unload();
        return false;
    }

    m_loaders[id] = new QPluginLoader(filePath, this);
    m_plugins[id] = plugin;
    m_enabledPlugins[id] = true;

    LOG_INFO("PluginManager", QString("Plugin loaded: %1 v%2").arg(id, plugin->version()));
    emit pluginLoaded(id, plugin->version());

    return true;
}

bool PluginManager::initializePlugin(IPlugin* plugin)
{
    EventBus* bus = EventBus::instance();
    ConfigManager* cfg = ConfigManager::instance();

    return plugin->initialize(bus, cfg);
}

void PluginManager::shutdownPlugin(IPlugin* plugin)
{
    plugin->shutdown();
}

void PluginManager::unloadPlugins()
{
    for (auto it = m_plugins.begin(); it != m_plugins.end(); ++it) {
        IPlugin* plugin = it.value();
        shutdownPlugin(plugin);
        emit pluginUnloaded(it.key());
        LOG_INFO("PluginManager", QString("Plugin unloaded: %1").arg(it.key()));
    }

    qDeleteAll(m_loaders);
    m_loaders.clear();
    m_plugins.clear();
    m_enabledPlugins.clear();
}

IPlugin* PluginManager::plugin(const QString& id) const
{
    return m_plugins.value(id, nullptr);
}

QList<IPlugin*> PluginManager::allPlugins() const
{
    return m_plugins.values();
}

QStringList PluginManager::loadedPluginIds() const
{
    return m_plugins.keys();
}

bool PluginManager::isPluginLoaded(const QString& id) const
{
    return m_plugins.contains(id);
}

QString PluginManager::pluginVersion(const QString& id) const
{
    IPlugin* plugin = m_plugins.value(id);
    return plugin ? plugin->version() : QString();
}

bool PluginManager::meetsDependencies(const QString& id) const
{
    IPlugin* plugin = m_plugins.value(id);
    if (!plugin) {
        return false;
    }

    QStringList deps = plugin->dependencies();
    for (const QString& dep : deps) {
        if (!m_plugins.contains(dep)) {
            return false;
        }
    }
    return true;
}

QStringList PluginManager::missingDependencies(const QString& id) const
{
    QStringList missing;
    IPlugin* plugin = m_plugins.value(id);

    if (plugin) {
        for (const QString& dep : plugin->dependencies()) {
            if (!m_plugins.contains(dep)) {
                missing.append(dep);
            }
        }
    }

    return missing;
}

void PluginManager::setPluginEnabled(const QString& id, bool enabled)
{
    m_enabledPlugins[id] = enabled;
}

bool PluginManager::isPluginEnabled(const QString& id) const
{
    return m_enabledPlugins.value(id, true);
}

}
