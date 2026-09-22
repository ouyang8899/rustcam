#include "ConfigManager.h"
#include "Logger.h"
#include <QFileInfo>
#include <QDir>
#include <QCoreApplication>

namespace rustcam {

ConfigManager::ConfigManager(QObject* parent)
    : QObject(parent)
{
    QString defaultPath = QCoreApplication::applicationDirPath() + "/config/rustcam.ini";
    QFileInfo fi(defaultPath);
    if (!fi.absoluteDir().exists()) {
        fi.absoluteDir().mkpath(".");
    }
    m_settings = new QSettings(defaultPath, QSettings::IniFormat, this);
    m_settings->setIniCodec("UTF-8");

    LOG_INFO("ConfigManager", QString("Config loaded from: %1").arg(defaultPath));
}

ConfigManager::~ConfigManager()
{
    sync();
}

ConfigManager* ConfigManager::instance()
{
    static ConfigManager inst;
    return &inst;
}

void ConfigManager::setValue(const QString& group, const QString& key, const QVariant& value)
{
    if (!group.isEmpty()) {
        m_settings->beginGroup(group);
    }
    m_settings->setValue(key, value);
    if (!group.isEmpty()) {
        m_settings->endGroup();
    }

    emit configChanged(group, key, value);
}

QVariant ConfigManager::getValue(const QString& group, const QString& key, const QVariant& defaultValue) const
{
    if (!group.isEmpty()) {
        m_settings->beginGroup(group);
    }
    QVariant value = m_settings->value(key, defaultValue);
    if (!group.isEmpty()) {
        m_settings->endGroup();
    }

    return value;
}

void ConfigManager::setGroup(const QString& group)
{
    m_settings->beginGroup(group);
}

QString ConfigManager::currentGroup() const
{
    return m_settings->group();
}

void ConfigManager::remove(const QString& group, const QString& key)
{
    if (!group.isEmpty()) {
        m_settings->beginGroup(group);
    }
    m_settings->remove(key);
    if (!group.isEmpty()) {
        m_settings->endGroup();
    }
}

void ConfigManager::clearGroup(const QString& group)
{
    if (group.isEmpty()) {
        m_settings->clear();
    } else {
        m_settings->beginGroup(group);
        m_settings->clear();
        m_settings->endGroup();
    }
}

QStringList ConfigManager::childGroups(const QString& group) const
{
    if (!group.isEmpty()) {
        m_settings->beginGroup(group);
    }
    QStringList groups = m_settings->childGroups();
    if (!group.isEmpty()) {
        m_settings->endGroup();
    }

    return groups;
}

QStringList ConfigManager::childKeys(const QString& group) const
{
    if (!group.isEmpty()) {
        m_settings->beginGroup(group);
    }
    QStringList keys = m_settings->childKeys();
    if (!group.isEmpty()) {
        m_settings->endGroup();
    }

    return keys;
}

bool ConfigManager::contains(const QString& group, const QString& key) const
{
    if (!group.isEmpty()) {
        m_settings->beginGroup(group);
    }
    bool exists = m_settings->contains(key);
    if (!group.isEmpty()) {
        m_settings->endGroup();
    }

    return exists;
}

void ConfigManager::load(const QString& filePath)
{
    delete m_settings;
    m_configFilePath = filePath;

    QFileInfo fi(filePath);
    if (fi.absoluteDir().exists() || fi.absoluteDir().mkpath(".")) {
        m_settings = new QSettings(filePath, QSettings::IniFormat, this);
        m_settings->setIniCodec("UTF-8");
        LOG_INFO("ConfigManager", QString("Config loaded from: %1").arg(filePath));
    } else {
        m_settings = new QSettings(QCoreApplication::applicationDirPath() + "/config/rustcam.ini",
                                  QSettings::IniFormat, this);
        LOG_WARNING("ConfigManager", QString("Failed to create dir for: %1, using default").arg(filePath));
    }
}

void ConfigManager::save(const QString& filePath)
{
    if (!filePath.isEmpty()) {
        delete m_settings;
        m_configFilePath = filePath;
        m_settings = new QSettings(filePath, QSettings::IniFormat, this);
        m_settings->setIniCodec("UTF-8");
    }

    m_settings->sync();
    LOG_INFO("ConfigManager", "Config saved");
}

void ConfigManager::sync()
{
    m_settings->sync();
}

QVariantMap ConfigManager::getAllValues(const QString& group) const
{
    QVariantMap result;

    if (!group.isEmpty()) {
        m_settings->beginGroup(group);
    }

    for (const QString& key : m_settings->childKeys()) {
        result[key] = m_settings->value(key);
    }

    if (!group.isEmpty()) {
        m_settings->endGroup();
    }

    return result;
}

}
