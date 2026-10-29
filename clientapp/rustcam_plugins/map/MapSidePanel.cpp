#include "MapSidePanel.h"
#include "core/DeviceTreeModel.h"
#include "common/Logger.h"
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QFrame>

namespace rustcam {

MapSidePanel::MapSidePanel(QWidget* parent)
    : QWidget(parent)
    , m_regionTree(nullptr)
    , m_groupTree(nullptr)
    , m_deviceTree(nullptr)
    , m_mapContainer(nullptr)
{
    QHBoxLayout* mainLayout = new QHBoxLayout(this);
    mainLayout->setContentsMargins(0, 0, 0, 0);
    mainLayout->setSpacing(0);

    m_tabWidget = new QTabWidget(this);
    m_tabWidget->setObjectName("mapTabs");
    m_tabWidget->setMinimumWidth(200);
    m_tabWidget->setMaximumWidth(300);

    m_regionTree = new DeviceTree(m_tabWidget);
    m_groupTree = new DeviceTree(m_tabWidget);
    m_deviceTree = new DeviceTree(m_tabWidget);

    m_tabWidget->addTab(m_regionTree, tr("Region"));
    m_tabWidget->addTab(m_groupTree, tr("Group"));
    m_tabWidget->addTab(m_deviceTree, tr("Device"));

    mainLayout->addWidget(m_tabWidget);

    m_mapContainer = new QFrame(this);
    m_mapContainer->setObjectName("mapContainer");
    m_mapContainer->setFrameShape(QFrame::NoFrame);

    QVBoxLayout* mapLayout = new QVBoxLayout(m_mapContainer);
    mapLayout->setContentsMargins(0, 0, 0, 0);
    mapLayout->setSpacing(0);

    mainLayout->addWidget(m_mapContainer, 1);

    connect(m_regionTree, &DeviceTree::itemDoubleClicked, this, &MapSidePanel::onDeviceItemDoubleClicked);
    connect(m_groupTree, &DeviceTree::itemDoubleClicked, this, &MapSidePanel::onDeviceItemDoubleClicked);
    connect(m_deviceTree, &DeviceTree::itemDoubleClicked, this, &MapSidePanel::onDeviceItemDoubleClicked);
}

MapSidePanel::~MapSidePanel() = default;

void MapSidePanel::setRegionTreeNodes(const QList<DeviceTreeNode::Ptr>& nodes) {
    m_regionTree->setNodes(nodes);
}

void MapSidePanel::setGroupTreeNodes(const QList<DeviceTreeNode::Ptr>& nodes) {
    m_groupTree->setNodes(nodes);
}

void MapSidePanel::setDeviceTreeNodes(const QList<DeviceTreeNode::Ptr>& nodes) {
    m_deviceTree->setNodes(nodes);
}

void MapSidePanel::onDeviceItemDoubleClicked(QVariantMap meta) {
    QString type = meta["type"].toString();
    if (type == "channel" || type == "device") {
        QString deviceTag = meta["device_tag"].toString();
        QString deviceName = meta["name"].toString();
        emit deviceSelected(deviceTag, deviceName);
    }
}

}
