#include "IPlugin.h"
#include <QFileInfo>
#include <QDir>
#include <QJsonDocument>
#include <QJsonObject>

namespace rustcam {

void IPlugin::setPluginPath(const QString& path)
{
    m_pluginPath = path;
    QDir pluginDir = QFileInfo(path).dir();

    QJsonObject config;
    QString configPath = pluginDir.absoluteFilePath("plugin.json");
    if (QFileInfo::exists(configPath)) {
        QFile file(configPath);
        if (file.open(QIODevice::ReadOnly)) {
            QJsonDocument doc = QJsonDocument::fromJson(file.readAll());
            config = doc.object();
            file.close();
        }
    }
    m_shortcut = config["shortcut"].toString();

    QString iconRelative = config["icon"].toString().trimmed();
    if (iconRelative.isEmpty() || iconRelative.startsWith(":/")
        || QDir::isAbsolutePath(iconRelative)) {
        iconRelative = QStringLiteral("icons/icon.svg");
    }
    QString iconPath = pluginDir.absoluteFilePath(iconRelative);
    if (QFileInfo::exists(iconPath)) {
        m_icon = QIcon(iconPath);
    }
}

}
