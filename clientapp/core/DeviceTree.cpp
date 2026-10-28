#include "DeviceTree.h"
#include "ThemeManager.h"
#include <QHeaderView>
#include <QMimeData>
#include <QDrag>
#include <QApplication>
#include <QPainter>
#include <QPalette>

namespace rustcam {

namespace {

/// QTreeView 的 branch（缩进/展开箭头）区域由 drawBranches 单独绘制，
/// 原生 QSS 的 branch:selected 在 Qt5 下不可靠，这里手动补齐整行选中背景。
class DeviceTreeView : public QTreeView {
public:
    explicit DeviceTreeView(QWidget* parent = nullptr) : QTreeView(parent) {}

protected:
    void drawBranches(QPainter* painter, const QRect& rect,
                      const QModelIndex& index) const override {
        if (selectionModel() && selectionModel()->isSelected(index)) {
            const bool active = isActiveWindow();
            const QColor bg = ThemeManager::instance()->getColor(
                active ? "selected_bg_active" : "selected_bg",
                palette().color(active ? QPalette::Active : QPalette::Inactive,
                                 QPalette::Highlight));
            if (bg.isValid()) {
                painter->fillRect(rect, bg);
            }
        }
        QTreeView::drawBranches(painter, rect, index);
    }
};

} // namespace

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

    m_tree = new DeviceTreeView();
    m_tree->setObjectName("deviceTreeView");
    m_model = new DeviceTreeModel(this);
    m_tree->setModel(m_model);
    m_tree->setHeaderHidden(true);
    m_tree->setEditTriggers(QTreeView::NoEditTriggers);
    m_tree->setDragEnabled(true);
    m_tree->setAcceptDrops(false);
    // 单列树：item 选中即可，branch 背景由 DeviceTreeView 补齐
    m_tree->setSelectionBehavior(QTreeView::SelectItems);
    m_tree->setItemDelegate(new FullRowSelectDelegate(m_tree));

    connect(m_tree, &QTreeView::doubleClicked, this, &DeviceTree::onItemDoubleClicked);
    connect(m_tree, &QTreeView::clicked, this, &DeviceTree::onItemClicked);

    mainLayout->addWidget(m_tree);
}

void DeviceTree::setNodes(const QList<DeviceTreeNode::Ptr>& nodes)
{
    QList<DeviceTreeNodeData*> rootNodes = buildTreeList(nodes, nullptr);
    m_model->setRootNodes(rootNodes);
    m_tree->expandAll();
}

QList<DeviceTreeNodeData*> DeviceTree::buildTreeList(const QList<DeviceTreeNode::Ptr>& nodes, DeviceTreeNodeData* parent)
{
    QList<DeviceTreeNodeData*> result;
    for (const DeviceTreeNode::Ptr& node : nodes) {
        DeviceTreeNodeData* dataNode = new DeviceTreeNodeData();
        dataNode->key = node->key;
        dataNode->name = node->name;
        dataNode->type = node->type;
        dataNode->metadata = node->metadata;
        dataNode->parent = parent;

        if (node->children.size() > 0) {
            dataNode->children = buildTreeList(node->children, dataNode);
        }

        result.append(dataNode);
    }
    return result;
}

bool DeviceTree::updateNodeState(const QString& nodeKey, const QVariantMap& state)
{
    Q_UNUSED(nodeKey)
    Q_UNUSED(state)
    return false;
}

void DeviceTree::onSearchTextChanged(const QString& text)
{
    Q_UNUSED(text)
}

void DeviceTree::onItemDoubleClicked(const QModelIndex& index)
{
    if (!index.isValid())
        return;

    DeviceTreeNodeData* node = m_model->nodeFromIndex(index);
    if (!node)
        return;

    QVariantMap meta;
    meta["key"] = node->key;
    meta["name"] = node->name;
    meta["type"] = node->type;
    meta.unite(node->metadata);

    emit itemDoubleClicked(meta);
}

void DeviceTree::onItemClicked(const QModelIndex& index)
{
    if (!index.isValid())
        return;

    DeviceTreeNodeData* node = m_model->nodeFromIndex(index);
    if (!node)
        return;

    QVariantMap meta;
    meta["key"] = node->key;
    meta["name"] = node->name;
    meta["type"] = node->type;
    meta.unite(node->metadata);

    emit itemClicked(meta);
}

}
