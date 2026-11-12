#include "PluginOrder.h"

#include <QFileInfo>
#include <QDir>
#include <QSet>
#include <algorithm>

namespace rustcam {

QStringList orderPluginIds(const QStringList& configured, const QStringList& available)
{
    QSet<QString> availableSet;
    for (const QString& id : available) {
        availableSet.insert(id);
    }

    QStringList ordered;
    QSet<QString> used;

    for (const QString& id : configured) {
        if (!availableSet.contains(id) || used.contains(id)) {
            continue;
        }
        ordered.append(id);
        used.insert(id);
    }

    for (const QString& id : available) {
        if (used.contains(id)) {
            continue;
        }
        ordered.append(id);
        used.insert(id);
    }

    return ordered;
}

QStringList defaultPluginIds()
{
    return QStringList({"live_video", "alarm", "playback", "map", "settings"});
}

QString pluginIdFromPath(const QString& filePath)
{
    QFileInfo fi(filePath);

    // 插件放在 rustcam_plugins/<插件 id>/ 目录下，目录名就是插件 id。
    QString dirName = fi.absoluteDir().dirName();
    if (!dirName.isEmpty() && dirName != "rustcam_plugins" && dirName != "plugins") {
        return dirName;
    }

    QString baseName = fi.completeBaseName();
    const QString prefix = "rustcam-";
    if (baseName.startsWith(prefix)) {
        return baseName.mid(prefix.size());
    }
    return baseName;
}

QStringList sortPluginFiles(const QStringList& files, const QStringList& configured)
{
    if (files.size() < 2) {
        return files;
    }

    QStringList ids;
    for (const QString& file : files) {
        ids.append(pluginIdFromPath(file));
    }
    const QStringList orderedIds = orderPluginIds(configured, ids);

    QStringList sorted = files;
    std::stable_sort(sorted.begin(), sorted.end(), [&orderedIds](const QString& a, const QString& b) {
        int indexA = orderedIds.indexOf(pluginIdFromPath(a));
        int indexB = orderedIds.indexOf(pluginIdFromPath(b));
        if (indexA < 0) indexA = orderedIds.size();
        if (indexB < 0) indexB = orderedIds.size();
        return indexA < indexB;
    });

    return sorted;
}

}
