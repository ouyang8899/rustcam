#pragma once

#include <QString>
#include <QVariantMap>
#include <QList>
#include<memory>
namespace rustcam {

struct DeviceTreeNode {
    using Ptr = std::shared_ptr<DeviceTreeNode>;
    QString key;
    QString name;
    QString type;
    QString parentKey;
    QVariantMap metadata;
    bool inRegion{ false };
    bool inGroup{ false };
    QList<DeviceTreeNode::Ptr> children;
};

}
