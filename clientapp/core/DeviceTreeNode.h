#pragma once

#include <QString>
#include <QVariantMap>
#include <QList>

namespace rustcam {

struct DeviceTreeNode {
    QString key;
    QString name;
    QString type;
    QString parentKey;
    QVariantMap metadata;
    QList<DeviceTreeNode> children;
};

}
