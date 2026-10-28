#include "DeviceTreeModel.h"
#include <QIcon>
#include <QJsonDocument>
#include <QMimeData>
#include <QVariant>

namespace rustcam {

DeviceTreeModel::DeviceTreeModel(QObject* parent)
    : QAbstractItemModel(parent)
    , m_rootNode(new DeviceTreeNodeData())
{
    m_rootNode->parent = nullptr;
    setupIcons();
}

DeviceTreeModel::~DeviceTreeModel()
{
    clear();
    delete m_rootNode;
}

void DeviceTreeModel::setupIcons()
{
    m_icons.resize(8);
    m_icons[0] = QIcon(":/icons/region.svg");
    m_icons[1] = QIcon(":/icons/group.svg");
    m_icons[2] = QIcon(":/icons/device.svg");
    m_icons[3] = QIcon(":/icons/channel.svg");
    m_icons[4] = QIcon(":/icons/device_online.svg");
    m_icons[5] = QIcon(":/icons/device_offline.svg");
    m_icons[6] = QIcon(":/icons/channel_online.svg");
    m_icons[7] = QIcon(":/icons/channel_offline.svg");
}

QModelIndex DeviceTreeModel::index(int row, int column, const QModelIndex& parent) const
{
    if (!hasIndex(row, column, parent))
        return QModelIndex();

    DeviceTreeNodeData* parentNode = m_rootNode;
    if (parent.isValid())
        parentNode = static_cast<DeviceTreeNodeData*>(parent.internalPointer());

    if (row >= 0 && row < parentNode->children.size())
        return createIndex(row, column, parentNode->children.at(row));

    return QModelIndex();
}

QModelIndex DeviceTreeModel::parent(const QModelIndex& index) const
{
    if (!index.isValid())
        return QModelIndex();

    DeviceTreeNodeData* node = static_cast<DeviceTreeNodeData*>(index.internalPointer());
    if (!node || node == m_rootNode)
        return QModelIndex();

    DeviceTreeNodeData* parentNode = node->parent;
    if (!parentNode || parentNode == m_rootNode)
        return QModelIndex();

    int row = parentNode->parent ? parentNode->parent->children.indexOf(parentNode) : 0;
    return createIndex(row, 0, parentNode);
}

int DeviceTreeModel::rowCount(const QModelIndex& parent) const
{
    if (!parent.isValid())
        return m_rootNode->children.size();

    DeviceTreeNodeData* node = static_cast<DeviceTreeNodeData*>(parent.internalPointer());
    if (!node)
        return 0;

    return node->children.size();
}

int DeviceTreeModel::columnCount(const QModelIndex& parent) const
{
    Q_UNUSED(parent)
    return 1;
}

QVariant DeviceTreeModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid())
        return QVariant();

    DeviceTreeNodeData* node = static_cast<DeviceTreeNodeData*>(index.internalPointer());
    if (!node)
        return QVariant();

    switch (role) {
    case Qt::DisplayRole:
    case Qt::EditRole:
        return node->name;

    case Qt::DecorationRole: {
        int iconIndex = -1;
        if (node->type == "region")
            iconIndex = 0;
        else if (node->type == "group")
            iconIndex = 1;
        else if (node->type == "device") {
            QString status = node->metadata.value("status").toString();
            iconIndex = (status == "Online") ? 4 : 5;
        }
        else if (node->type == "channel") {
            QString status = node->metadata.value("status").toString();
            iconIndex = (status == "Online") ? 6 : 7;
        }

        if (iconIndex >= 0 && iconIndex < m_icons.size() && !m_icons[iconIndex].isNull())
            return m_icons[iconIndex];
        return QVariant();
    }

    case KeyRole:
        return node->key;

    case NameRole:
        return node->name;

    case TypeRole:
        return node->type;

    case MetadataRole:
        return node->metadata;

    case StatusRole:
        return node->metadata.value("status");

    default:
        return QVariant();
    }
}

bool DeviceTreeModel::setData(const QModelIndex& index, const QVariant& value, int role)
{
    if (!index.isValid())
        return false;

    DeviceTreeNodeData* node = static_cast<DeviceTreeNodeData*>(index.internalPointer());
    if (!node)
        return false;

    switch (role) {
    case Qt::EditRole:
        node->name = value.toString();
        break;
    case NameRole:
        node->name = value.toString();
        break;
    default:
        return false;
    }

    emit dataChanged(index, index, {role});
    return true;
}

Qt::ItemFlags DeviceTreeModel::flags(const QModelIndex& index) const
{
    if (!index.isValid())
        return Qt::NoItemFlags;

    Qt::ItemFlags itemFlags = Qt::ItemIsEnabled | Qt::ItemIsSelectable;
    DeviceTreeNodeData* node = static_cast<DeviceTreeNodeData*>(index.internalPointer());
    if (node && node->type == "channel") {
        itemFlags |= Qt::ItemIsDragEnabled;
    }
    return itemFlags;
}

QStringList DeviceTreeModel::mimeTypes() const
{
    return { QStringLiteral("application/x-rustcam-device") };
}

QMimeData* DeviceTreeModel::mimeData(const QModelIndexList& indexes) const
{
    if (indexes.isEmpty()) {
        return nullptr;
    }

    DeviceTreeNodeData* node = static_cast<DeviceTreeNodeData*>(indexes.first().internalPointer());
    if (!node) {
        return nullptr;
    }

    QVariantMap meta;
    meta["key"] = node->key;
    meta["name"] = node->name;
    meta["type"] = node->type;
    meta.unite(node->metadata);

    QMimeData* mime = new QMimeData();
    mime->setData(QStringLiteral("application/x-rustcam-device"),
                  QJsonDocument::fromVariant(meta).toJson(QJsonDocument::Compact));
    return mime;
}

QVariant DeviceTreeModel::headerData(int section, Qt::Orientation orientation, int role) const
{
    Q_UNUSED(section)
    Q_UNUSED(orientation)
    Q_UNUSED(role)
    return QVariant();
}

void DeviceTreeModel::setRootNodes(const QList<DeviceTreeNodeData*>& nodes)
{
    beginResetModel();

    for (DeviceTreeNodeData* child : m_rootNode->children) {
        deleteNode(child);
    }
    m_rootNode->children.clear();

    for (DeviceTreeNodeData* node : nodes) {
        node->parent = m_rootNode;
        m_rootNode->children.append(node);
    }

    endResetModel();
}

void DeviceTreeModel::clear()
{
    beginResetModel();

    for (DeviceTreeNodeData* child : m_rootNode->children) {
        deleteNode(child);
    }
    m_rootNode->children.clear();

    endResetModel();
}

DeviceTreeNodeData* DeviceTreeModel::nodeFromIndex(const QModelIndex& index) const
{
    if (!index.isValid())
        return nullptr;

    return static_cast<DeviceTreeNodeData*>(index.internalPointer());
}

QModelIndex DeviceTreeModel::indexFromNode(DeviceTreeNodeData* node) const
{
    if (!node || node == m_rootNode)
        return QModelIndex();

    int row = node->parent ? node->parent->children.indexOf(node) : 0;
    return createIndex(row, 0, node);
}

DeviceTreeNodeData* DeviceTreeModel::createNode(const QString& key, const QString& name, const QString& type, const QVariantMap& metadata)
{
    DeviceTreeNodeData* node = new DeviceTreeNodeData();
    node->key = key;
    node->name = name;
    node->type = type;
    node->metadata = metadata;
    node->parent = nullptr;
    return node;
}

void DeviceTreeModel::deleteNode(DeviceTreeNodeData* node)
{
    for (DeviceTreeNodeData* child : node->children) {
        deleteNode(child);
    }
    node->children.clear();
    delete node;
}

void DeviceTreeModel::addDeviceToRegion(const QString& regionCode, DeviceTreeNodeData* device)
{
    DeviceTreeNodeData* parentNode = nullptr;

    QList<DeviceTreeNodeData*> stack = m_rootNode->children;
    while (!stack.isEmpty()) {
        DeviceTreeNodeData* node = stack.takeFirst();
        if (node->type == "region" && node->key == regionCode) {
            parentNode = node;
            break;
        }
        stack.append(node->children);
    }

    if (!parentNode)
        return;

    int row = parentNode->children.size();
    beginInsertRows(indexFromNode(parentNode), row, row);
    device->parent = parentNode;
    parentNode->children.append(device);
    endInsertRows();
}

void DeviceTreeModel::addDeviceToGroup(const QString& groupId, DeviceTreeNodeData* device)
{
    DeviceTreeNodeData* parentNode = nullptr;

    QList<DeviceTreeNodeData*> stack = m_rootNode->children;
    while (!stack.isEmpty()) {
        DeviceTreeNodeData* node = stack.takeFirst();
        if (node->type == "group" && node->key == groupId) {
            parentNode = node;
            break;
        }
        stack.append(node->children);
    }

    if (!parentNode)
        return;

    int row = parentNode->children.size();
    beginInsertRows(indexFromNode(parentNode), row, row);
    device->parent = parentNode;
    parentNode->children.append(device);
    endInsertRows();
}

void DeviceTreeModel::addChannelToDevice(const QString& deviceTag, DeviceTreeNodeData* channel)
{
    DeviceTreeNodeData* parentNode = nullptr;

    QList<DeviceTreeNodeData*> stack = m_rootNode->children;
    while (!stack.isEmpty()) {
        DeviceTreeNodeData* node = stack.takeFirst();
        if (node->type == "device" && node->key == deviceTag) {
            parentNode = node;
            break;
        }
        stack.append(node->children);
    }

    if (!parentNode)
        return;

    int row = parentNode->children.size();
    beginInsertRows(indexFromNode(parentNode), row, row);
    channel->parent = parentNode;
    parentNode->children.append(channel);
    endInsertRows();
}

}
