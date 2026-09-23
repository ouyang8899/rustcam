#include "DeviceTree.h"
#include <QHeaderView>
#include <QMimeData>
#include <QDrag>
#include <QApplication>

namespace rustcam {

DeviceTree::DeviceTree(QWidget* parent)
    : QWidget(parent)
{
    setupUi();
}

DeviceTree::~DeviceTree() = default;

void DeviceTree::setupUi()
{
    QVBoxLayout* mainLayout = new QVBoxLayout(this);
    mainLayout->setContentsMargins(4, 4, 4, 4);
    mainLayout->setSpacing(4);

    QHBoxLayout* toolbarLayout = new QHBoxLayout();
    toolbarLayout->setSpacing(4);

    m_searchEdit = new QLineEdit();
    m_searchEdit->setPlaceholderText(tr("Search..."));
    m_searchEdit->setMaximumWidth(150);
    connect(m_searchEdit, &QLineEdit::textChanged, this, &DeviceTree::onSearchTextChanged);
    toolbarLayout->addWidget(m_searchEdit);
    toolbarLayout->addStretch();

    mainLayout->addLayout(toolbarLayout);

    m_tree = new QTreeView();
    m_model = new QStandardItemModel(this);
    m_tree->setModel(m_model);
    m_tree->setHeaderHidden(true);
    m_tree->setEditTriggers(QTreeView::NoEditTriggers);
    m_tree->setDragEnabled(true);
    m_tree->setAcceptDrops(false);
    m_tree->setSelectionBehavior(QTreeView::SelectRows);

    connect(m_tree, &QTreeView::doubleClicked, [this](const QModelIndex& index) {
        if (!index.isValid()) return;
        QStandardItem* item = m_model->itemFromIndex(index);
        if (!item) return;

        QVariantMap meta = item->data(Qt::UserRole).toMap();
        if (meta.isEmpty()) return;

        emit itemDoubleClicked(meta);
    });

    connect(m_tree, &QTreeView::pressed, [this](const QModelIndex& index) {
        if (!index.isValid()) return;
        QStandardItem* item = m_model->itemFromIndex(index);
        if (!item) return;

        Qt::MouseButtons buttons = QApplication::mouseButtons();
        if (buttons & Qt::LeftButton && !(QApplication::keyboardModifiers() & Qt::ShiftModifier)) {
            QMimeData* mimeData = new QMimeData();
            QVariantMap meta = item->data(Qt::UserRole).toMap();
            mimeData->setData("application/x-devicetree-node", meta["key"].toByteArray());
            QDrag* drag = new QDrag(this);
            drag->setMimeData(mimeData);
            drag->exec();
        }
    });

    mainLayout->addWidget(m_tree);
}

void DeviceTree::setNodes(const QList<DeviceTreeNode>& nodes)
{
    m_keyToItem.clear();
    m_model->clear();

    QStandardItem* rootItem = m_model->invisibleRootItem();
    buildTree(nodes, rootItem);

    m_tree->expandAll();
}

void DeviceTree::buildTree(const QList<DeviceTreeNode>& nodes, QStandardItem* parent)
{
    for (const DeviceTreeNode& node : nodes) {
        QStandardItem* item = new QStandardItem(node.name);
        item->setEditable(false);

        QVariantMap meta;
        meta["key"] = node.key;
        meta["name"] = node.name;
        meta["type"] = node.type;
        meta["parentKey"] = node.parentKey;
        meta.unite(node.metadata);
        item->setData(meta, Qt::UserRole);

        m_keyToItem[node.key] = item;

        if (node.type == "device" || node.type == "channel") {
            if (node.metadata.contains("icon")) {
                item->setIcon(QIcon(node.metadata["icon"].toString()));
            }
        }

        parent->appendRow(item);

        if (!node.children.isEmpty()) {
            buildTree(node.children, item);
        }
    }
}

bool DeviceTree::updateNodeState(const QString& nodeKey, const QVariantMap& state)
{
    QStandardItem* item = m_keyToItem.value(nodeKey);
    if (!item) {
        emit nodeNotFound(nodeKey);
        return false;
    }

    QVariantMap meta = item->data(Qt::UserRole).toMap();
    meta.unite(state);
    item->setData(meta, Qt::UserRole);

    applyNodeState(item, state);
    emit nodeStateChanged(nodeKey, state);
    return true;
}

void DeviceTree::applyNodeState(QStandardItem* item, const QVariantMap& state)
{
    if (state.contains("icon")) {
        item->setIcon(QIcon(state["icon"].toString()));
    }
    if (state.contains("foreground")) {
        item->setForeground(QBrush(QColor(state["foreground"].toString())));
    }
    if (state.contains("background")) {
        item->setBackground(QBrush(QColor(state["background"].toString())));
    }
}

void DeviceTree::onSearchTextChanged(const QString& text)
{
    if (text.isEmpty()) {
        for (int i = 0; i < m_model->rowCount(); ++i) {
            m_tree->setRowHidden(i, QModelIndex(), false);
        }
        return;
    }

    for (int i = 0; i < m_model->rowCount(); ++i) {
        bool hasMatch = false;
        filterTreeItems(m_model->item(i), text, hasMatch);
        m_tree->setRowHidden(i, QModelIndex(), !hasMatch);
    }
}

bool DeviceTree::filterTreeItems(QStandardItem* item, const QString& text, bool& hasMatch)
{
    hasMatch = false;

    if (item->text().contains(text, Qt::CaseInsensitive)) {
        hasMatch = true;
    }

    for (int i = 0; i < item->rowCount(); ++i) {
        bool childMatch = false;
        if (filterTreeItems(item->child(i), text, childMatch)) {
            hasMatch = true;
        }
    }

    if (hasMatch) {
        expandParents(item);
    }

    return hasMatch;
}

void DeviceTree::expandParents(QStandardItem* item)
{
    QModelIndex index = item->index();
    while (index.isValid()) {
        m_tree->expand(index);
        index = index.parent();
    }
}

QStandardItem* DeviceTree::findItemByKey(QStandardItem* parent, const QString& key)
{
    if (m_keyToItem.contains(key)) {
        return m_keyToItem.value(key);
    }

    for (int i = 0; i < parent->rowCount(); ++i) {
        QStandardItem* child = parent->child(i);
        QStandardItem* found = findItemByKey(child, key);
        if (found) return found;
    }

    return nullptr;
}

}
