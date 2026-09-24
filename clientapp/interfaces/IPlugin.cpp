#include "IPlugin.h"
#include <QFileInfo>
#include <QDir>
#include <QJsonDocument>
#include <QJsonObject>

namespace rustcam {

void IPlugin::setPluginPath(const QString& path)
{
    m_pluginPath = path;
    QFileInfo fi(path);

    QString iconPath = fi.dir().absoluteFilePath("icons/icon.svg");
    if (QFileInfo::exists(iconPath)) {
        m_icon = QIcon(iconPath);
    }

    QString configPath = fi.dir().absoluteFilePath("plugin.json");
    if (QFileInfo::exists(configPath)) {
        QFile file(configPath);
        if (file.open(QIODevice::ReadOnly)) {
            QJsonDocument doc = QJsonDocument::fromJson(file.readAll());
            QJsonObject obj = doc.object();
            m_shortcut = obj["shortcut"].toString();
            file.close();
        }
    }
}

}
