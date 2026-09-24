#pragma once

#include <QAbstractItemModel>
#include <QIcon>
#include <QModelIndex>
#include <QVariant>
#include <QList>
#include <QString>
#include <QVariantMap>
#include <QVector>
#include "CoreExport.h"

namespace rustcam {

struct DeviceTreeNodeData {
    QString key;
    QString name;
    QString type;
    QVariantMap metadata;
    QList<DeviceTreeNodeData*> children;
    DeviceTreeNodeData* parent;
};

class RUSTCAM_CORE_API DeviceTreeModel : public QAbstractItemModel {
    Q_OBJECT

public:
    enum Roles {
        KeyRole = Qt::UserRole + 1,
        NameRole,
        TypeRole,
        MetadataRole
    };

    explicit DeviceTreeModel(QObject* parent = nullptr);
    ~DeviceTreeModel();

    QModelIndex index(int row, int column, const QModelIndex& parent = QModelIndex()) const override;
    QModelIndex parent(const QModelIndex& index) const override;
    int rowCount(const QModelIndex& parent = QModelIndex()) const override;
    int columnCount(const QModelIndex& parent = QModelIndex()) const override;
    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
    bool setData(const QModelIndex& index, const QVariant& value, int role = Qt::EditRole) override;
    Qt::ItemFlags flags(const QModelIndex& index) const override;
    QVariant headerData(int section, Qt::Orientation orientation, int role = Qt::DisplayRole) const override;

    void setRootNodes(const QList<DeviceTreeNodeData*>& nodes);
    void clear();
    DeviceTreeNodeData* nodeFromIndex(const QModelIndex& index) const;
    QModelIndex indexFromNode(DeviceTreeNodeData* node) const;

    void addDeviceToRegion(const QString& regionCode, DeviceTreeNodeData* device);
    void addDeviceToGroup(const QString& groupId, DeviceTreeNodeData* device);
    void addChannelToDevice(const QString& deviceTag, DeviceTreeNodeData* channel);

signals:
    void nodeDoubleClicked(const QVariantMap& meta);

private:
    DeviceTreeNodeData* m_rootNode;
    QVector<QIcon> m_icons;

    void setupIcons();
    DeviceTreeNodeData* createNode(const QString& key, const QString& name, const QString& type, const QVariantMap& metadata = QVariantMap());
    void deleteNode(DeviceTreeNodeData* node);
};

}
