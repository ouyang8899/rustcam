#include "DataManager.h"
#include "ServerApi.h"
#include "EventBus.h"
#include "common/Logger.h"
#include <unordered_map>

namespace rustcam {

DataManager* DataManager::instance() {
    static DataManager instance;
    return &instance;
}

DataManager::DataManager(QObject* parent)
    : QObject(parent)
{
    EventBus::instance()->subscribe("session/loggedIn", [this](const QString& topic, EventDataPtr) {
        Q_UNUSED(topic)
        LOG_INFO("DataManager", "Login detected, loading data...");
        loadAll();
    });
}

DataManager::~DataManager() {
    if (m_watcher) {
        m_watcher->waitForFinished();
        delete m_watcher;
    }
}

void DataManager::loadAll() {
    QMutexLocker locker(&m_mutex);
    if (m_loading) {
        return;
    }
    m_loading = true;
    emit loadStarted();

    QtConcurrent::run([this]() {
        loadRegionsTree();
        loadGroupsTree();
        loadDevicesAndChannels();
        buildDeviceTrees();

        QMetaObject::invokeMethod(this, [this]() {
            QMutexLocker locker(&m_mutex);
            m_loaded = true;
            m_loading = false;
            emit loadFinished(true);
            emit dataChanged();
            LOG_INFO("DataManager", "All data loaded successfully");
        });
    });
}

void DataManager::reload() {
    QMutexLocker locker(&m_mutex);
    clear();
    loadAll();
}

void DataManager::clear() {
    QMutexLocker locker(&m_mutex);
    m_loaded = false;
    m_layoutsLoaded = false;
    m_regionNodes.clear();
    m_groupNodes.clear();
    m_deviceNodes.clear();
    m_layouts.clear();
    m_regionsData = QJsonArray();
    m_groupsData = QJsonArray();
    m_devicesData = QJsonArray();
    m_channelsData = QJsonArray();
}

void DataManager::loadLayouts() {
    QJsonObject response;
    QString error;
    if (ServerApi::instance()->getSync("/api/v1/layouts", response, error)) {
        QJsonArray data = response["data"].toArray();
        for (const QJsonValue& v : data) {
            m_layouts.append(PlayerLayout::fromJson(v.toObject()));
        }
        LOG_INFO("DataManager", QString("Loaded layouts: %1").arg(m_layouts.size()));
    } else {
        LOG_ERROR("DataManager", QString("Failed to load layouts: %1").arg(error));
    }

    if (m_layouts.isEmpty()) {
        PlayerLayout layout1x1;
        layout1x1.id = 1;
        layout1x1.name = "1x1";
        layout1x1.rows = 1;
        layout1x1.cols = 1;
        layout1x1.layoutJson.append(LayoutItem{"cell-0", 0, 0, 1, 1});

        PlayerLayout layout2x2;
        layout2x2.id = 2;
        layout2x2.name = "2x2";
        layout2x2.rows = 2;
        layout2x2.cols = 2;
        layout2x2.layoutJson.append(LayoutItem{"cell-0", 0, 0, 1, 1});
        layout2x2.layoutJson.append(LayoutItem{"cell-1", 0, 1, 1, 1});
        layout2x2.layoutJson.append(LayoutItem{"cell-2", 1, 0, 1, 1});
        layout2x2.layoutJson.append(LayoutItem{"cell-3", 1, 1, 1, 1});

        PlayerLayout layout3x3;
        layout3x3.id = 3;
        layout3x3.name = "3x3";
        layout3x3.rows = 3;
        layout3x3.cols = 3;
        for (int i = 0; i < 9; ++i) {
            LayoutItem item;
            item.id = QString("cell-%1").arg(i);
            item.row = i / 3;
            item.col = i % 3;
            item.rowSpan = 1;
            item.colSpan = 1;
            layout3x3.layoutJson.append(item);
        }

        m_layouts.append(layout1x1);
        m_layouts.append(layout2x2);
        m_layouts.append(layout3x3);
        LOG_INFO("DataManager", "Using default layouts");
    }

    m_layoutsLoaded = true;
    emit layoutsLoaded();
}

void DataManager::loadRegionsTree() {
    QJsonObject response;
    QString error;
    if (ServerApi::instance()->getSync("/api/v1/regions/tree", response, error)) {
        m_regionsData = response["data"].toArray();
        LOG_INFO("DataManager", QString("Loaded regions: %1").arg(m_regionsData.size()));
    } else {
        LOG_ERROR("DataManager", QString("Failed to load regions: %1").arg(error));
    }
}

void DataManager::loadGroupsTree() {
    QJsonObject response;
    QString error;
    if (ServerApi::instance()->getSync("/api/v1/groups/tree", response, error)) {
        m_groupsData = response["data"].toArray();
        LOG_INFO("DataManager", QString("Loaded groups: %1").arg(m_groupsData.size()));
    } else {
        LOG_ERROR("DataManager", QString("Failed to load groups: %1").arg(error));
    }
}

void DataManager::loadDevicesAndChannels() {
    QJsonObject devicesResponse;
    QString devicesError;
    if (ServerApi::instance()->getSync("/api/v1/devices?limit=500", devicesResponse, devicesError)) {
        m_devicesData = devicesResponse["data"].toObject().value("items").toArray();
        LOG_INFO("DataManager", QString("Loaded devices: %1").arg(m_devicesData.size()));
    } else {
        LOG_ERROR("DataManager", QString("Failed to load devices: %1").arg(devicesError));
    }

    QJsonObject channelsResponse;
    QString channelsError;
    if (ServerApi::instance()->getSync("/api/v1/channels?limit=500", channelsResponse, channelsError)) {
        m_channelsData = channelsResponse["data"].toObject().value("items").toArray();
        LOG_INFO("DataManager", QString("Loaded channels: %1").arg(m_channelsData.size()));
    } else {
        LOG_ERROR("DataManager", QString("Failed to load channels: %1").arg(channelsError));
    }
}

void DataManager::buildDeviceTrees() {
    QList<DeviceTreeNode::Ptr> regionNodes;
    QList<DeviceTreeNode::Ptr> groupNodes;

    QMap<QString, QList<DeviceTreeNode::Ptr>> regionDeviceMap;
    QMap<QString, QList<DeviceTreeNode::Ptr>> groupDeviceMap;
    std::unordered_map<QString, DeviceTreeNode::Ptr> allDeviceMap;

    for (const QJsonValue& v : m_devicesData) {
        QJsonObject obj = v.toObject();
        DeviceTreeNode::Ptr deviceNode = std::make_shared<DeviceTreeNode>();
        deviceNode->key = obj["device_tag"].toString();
        deviceNode->name = obj["name"].toString();
        deviceNode->type = "device";
        if (deviceNode->name.isEmpty()) {
            continue;
        }
        QVariantMap meta;
        meta["device_tag"] = obj["device_tag"].toVariant();
        meta["host"] = obj["host"].toVariant();
        meta["port"] = obj["port"].toVariant();
        meta["status"] = obj["status"].toString();
        deviceNode->metadata = meta;

        QString regionCode = obj["region_code"].toString();
        if (!regionCode.isEmpty()) {
            deviceNode->inRegion = true;
            regionDeviceMap[regionCode].append(deviceNode);
        }

        QString groupId = QString::number(obj["group_id"].toInt());
        if (!groupId.isEmpty() && obj["group_id"].toInt() > 0) {
            deviceNode->inGroup = true;
            groupDeviceMap[groupId].append(deviceNode);
        }
        allDeviceMap[deviceNode->key] = deviceNode;
    }

    for (const QJsonValue& v : m_channelsData) {
        QJsonObject obj = v.toObject();
        DeviceTreeNode::Ptr channelNode = std::make_shared<DeviceTreeNode>();
        channelNode->key = obj["device_tag"].toString() + "_" + obj["channel_tag"].toString();
        channelNode->name = obj["name"].toString();
        channelNode->type = "channel";
        if (channelNode->name.isEmpty()) {
            continue;
        }
        QVariantMap meta;
        meta["device_tag"] = obj["device_tag"].toVariant();
        meta["channel_tag"] = obj["channel_tag"].toVariant();
        meta["host"] = obj["ip_address"].toString();
        meta["port"] = obj["port"].toVariant();
        meta["status"] = obj["status"].toString();
        channelNode->metadata = meta;

        QString deviceTag = obj["device_tag"].toString();
        auto it = allDeviceMap.find(deviceTag);
        if (it != allDeviceMap.end()) {
            it->second->children.append(channelNode);
        }
    }

    for (const QJsonValue& v : m_regionsData) {
        auto node = parseRegionNode(v.toObject(), regionDeviceMap);
        regionNodes.append(node);
    }

    for (const QJsonValue& v : m_groupsData) {
        groupNodes.append(parseGroupNode(v.toObject(), groupDeviceMap));
    }

    for (auto& d : allDeviceMap) {
        if (!d.second->inGroup) {
            groupNodes.append(d.second);
        }
        if (!d.second->inRegion) {
            regionNodes.append(d.second);
        }
    }

    m_regionNodes = regionNodes;
    m_groupNodes = groupNodes;
    for (const auto& d : allDeviceMap) {
        m_deviceNodes.append(d.second);
    }

    LOG_INFO("DataManager", QString("Built trees: regionNodes=%1, groupNodes=%2, deviceNodes=%3")
        .arg(m_regionNodes.size()).arg(m_groupNodes.size()).arg(m_deviceNodes.size()));
}

DeviceTreeNode::Ptr DataManager::parseRegionNode(const QJsonObject& obj,
    const QMap<QString, QList<DeviceTreeNode::Ptr>>& regionDeviceMap)
{
    DeviceTreeNode::Ptr node = std::make_shared<DeviceTreeNode>();
    node->key = obj["code"].toString();
    QString name = obj["name"].toString();
    int deviceCount = obj["device_count"].toInt();
    if (deviceCount > 0) {
        name = name + " (" + QString::number(deviceCount) + ")";
    }
    node->name = name;
    node->type = "region";

    QJsonArray children = obj["children"].toArray();
    for (const QJsonValue& child : children) {
        node->children.append(parseRegionNode(child.toObject(), regionDeviceMap));
    }
    if (regionDeviceMap.contains(node->key)) {
        node->children.append(regionDeviceMap[node->key]);
    }
    return node;
}

DeviceTreeNode::Ptr DataManager::parseGroupNode(const QJsonObject& obj,
    const QMap<QString, QList<DeviceTreeNode::Ptr>>& groupDeviceMap)
{
    DeviceTreeNode::Ptr node = std::make_shared<DeviceTreeNode>();
    node->key = QString::number(obj["id"].toInt());
    QString name = obj["name"].toString();
    int deviceCount = obj["device_count"].toInt();
    if (deviceCount > 0) {
        name = name + " (" + QString::number(deviceCount) + ")";
    }
    node->name = name;
    node->type = "group";

    QJsonArray children = obj["children"].toArray();
    for (const QJsonValue& child : children) {
        node->children.append(parseGroupNode(child.toObject(), groupDeviceMap));
    }
    if (groupDeviceMap.contains(node->key)) {
        node->children.append(groupDeviceMap[node->key]);
    }
    return node;
}

}