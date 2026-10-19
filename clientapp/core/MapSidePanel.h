#pragma once

#include <QWidget>
#include <QList>
#include <QVariantMap>
#include <QTabWidget>
#include <QJsonArray>
#include "CoreExport.h"
#include "DeviceTree.h"

namespace rustcam {

class RUSTCAM_CORE_API MapSidePanel : public QWidget {
    Q_OBJECT
public:
    explicit MapSidePanel(QWidget* parent = nullptr);
    ~MapSidePanel();

    DeviceTree* regionTree() const { return m_regionTree; }
    DeviceTree* groupTree() const { return m_groupTree; }
    DeviceTree* deviceTree() const { return m_deviceTree; }

    void setRegionTreeNodes(const QList<DeviceTreeNode::Ptr>& nodes);
    void setGroupTreeNodes(const QList<DeviceTreeNode::Ptr>& nodes);
    void setDeviceTreeNodes(const QList<DeviceTreeNode::Ptr>& nodes);

    QWidget* mapContainer() const { return m_mapContainer; }

signals:
    void deviceSelected(const QString& deviceId, const QString& deviceName);

private slots:
    void onDeviceItemDoubleClicked(QVariantMap meta);

private:
    QTabWidget* m_tabWidget;
    DeviceTree* m_regionTree;
    DeviceTree* m_groupTree;
    DeviceTree* m_deviceTree;
    QFrame* m_mapContainer;
};

}
