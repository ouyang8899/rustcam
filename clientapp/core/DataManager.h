#pragma once

#include <QObject>
#include <QList>
#include <QMap>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QMutex>
#include <QtConcurrent>
#include <QFutureWatcher>
#include "CoreExport.h"
#include "DeviceTreeNode.h"
#include "PlayerLayout.h"

namespace rustcam {

class RUSTCAM_CORE_API DataManager : public QObject {
    Q_OBJECT
public:
    static DataManager* instance();

    bool isLoaded() const { return m_loaded; }
    bool isLoading() const { return m_loading; }
    bool isLayoutsLoaded() const { return m_layoutsLoaded; }

    const QList<DeviceTreeNode::Ptr>& regionTree() const { return m_regionNodes; }
    const QList<DeviceTreeNode::Ptr>& groupTree() const { return m_groupNodes; }
    const QList<DeviceTreeNode::Ptr>& deviceList() const { return m_deviceNodes; }
    const QJsonArray& regionsData() const { return m_regionsData; }
    const QJsonArray& groupsData() const { return m_groupsData; }
    const QJsonArray& devicesData() const { return m_devicesData; }
    const QJsonArray& channelsData() const { return m_channelsData; }
    const QList<PlayerLayout>& layouts() const { return m_layouts; }

    void loadAll();
    void loadLayouts();
    void reload();
    void clear();

signals:
    void loadStarted();
    void loadFinished(bool success);
    void layoutsLoaded();
    void dataChanged();

private:
    explicit DataManager(QObject* parent = nullptr);
    ~DataManager();
    Q_DISABLE_COPY(DataManager)

    void loadRegionsTree();
    void loadGroupsTree();
    void loadDevicesAndChannels();
    void buildDeviceTrees();

    DeviceTreeNode::Ptr parseRegionNode(const QJsonObject& obj, const QMap<QString, QList<DeviceTreeNode::Ptr>>& regionDeviceMap);
    DeviceTreeNode::Ptr parseGroupNode(const QJsonObject& obj, const QMap<QString, QList<DeviceTreeNode::Ptr>>& groupDeviceMap);

    bool m_loaded = false;
    bool m_loading = false;
    bool m_layoutsLoaded = false;
    mutable QMutex m_mutex;

    QList<DeviceTreeNode::Ptr> m_regionNodes;
    QList<DeviceTreeNode::Ptr> m_groupNodes;
    QList<DeviceTreeNode::Ptr> m_deviceNodes;

    QJsonArray m_regionsData;
    QJsonArray m_groupsData;
    QJsonArray m_devicesData;
    QJsonArray m_channelsData;
    QList<PlayerLayout> m_layouts;

    QFutureWatcher<void>* m_watcher = nullptr;
};

}