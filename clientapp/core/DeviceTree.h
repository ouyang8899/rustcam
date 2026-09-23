#pragma once

#include <QWidget>
#include <QMap>
#include <QStandardItemModel>
#include <QTreeView>
#include <QLineEdit>
#include <QComboBox>
#include <QVBoxLayout>
#include <QHBoxLayout>
#include "DeviceTreeNode.h"
#include "CoreExport.h"

namespace rustcam {

class RUSTCAM_CORE_API DeviceTree : public QWidget {
    Q_OBJECT
public:
    explicit DeviceTree(QWidget* parent = nullptr);
    ~DeviceTree();

    void setNodes(const QList<DeviceTreeNode>& nodes);
    bool updateNodeState(const QString& nodeKey, const QVariantMap& state);

signals:
    void itemDoubleClicked(QVariantMap meta);
    void itemDragged(QVariantMap meta);
    void nodeStateChanged(const QString& nodeKey, const QVariantMap& state);
    void nodeNotFound(const QString& nodeKey);

private slots:
    void onSearchTextChanged(const QString& text);

private:
    void setupUi();
    void buildTree(const QList<DeviceTreeNode>& nodes, QStandardItem* parent);
    QStandardItem* findItemByKey(QStandardItem* parent, const QString& key);
    void applyNodeState(QStandardItem* item, const QVariantMap& state);
    bool filterTreeItems(QStandardItem* item, const QString& text, bool& hasMatch);
    void expandParents(QStandardItem* item);

    QMap<QString, QStandardItem*> m_keyToItem;
    QStandardItemModel* m_model;
    QTreeView* m_tree;
    QLineEdit* m_searchEdit;
};

}
