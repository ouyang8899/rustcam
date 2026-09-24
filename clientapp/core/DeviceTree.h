#pragma once

#include <QWidget>
#include <QTreeView>
#include <QLineEdit>
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QHeaderView>
#include "DeviceTreeModel.h"
#include "DeviceTreeNode.h"
#include "CoreExport.h"
#include "FullRowSelectDelegate.h"

namespace rustcam {

class RUSTCAM_CORE_API DeviceTree : public QWidget {
    Q_OBJECT
public:
    explicit DeviceTree(QWidget* parent = nullptr);
    ~DeviceTree();

    void setNodes(const QList<DeviceTreeNode::Ptr>& nodes);
    bool updateNodeState(const QString& nodeKey, const QVariantMap& state);

    DeviceTreeModel* model() const { return m_model; }

signals:
    void itemDoubleClicked(QVariantMap meta);
    void itemClicked(QVariantMap meta);
    void nodeStateChanged(const QString& nodeKey, const QVariantMap& state);
    void nodeNotFound(const QString& nodeKey);

private slots:
    void onSearchTextChanged(const QString& text);
    void onItemDoubleClicked(const QModelIndex& index);
    void onItemClicked(const QModelIndex& index);

private:
    void setupUi();
    QList<DeviceTreeNodeData*> buildTreeList(const QList<DeviceTreeNode::Ptr>& nodes, DeviceTreeNodeData* parent);

    DeviceTreeModel* m_model;
    QTreeView* m_tree;
    QLineEdit* m_searchEdit;
};

}
