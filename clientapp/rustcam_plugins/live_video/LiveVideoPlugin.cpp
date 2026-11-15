#include "LiveVideoPlugin.h"
#include "core/EventBus.h"
#include "core/ServerApi.h"
#include "core/DeviceTreeNode.h"
#include "common/Logger.h"
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QGridLayout>
#include <QJsonArray>
#include <QtConcurrent>

namespace rustcam {

bool LiveVideoPlugin::initialize(EventBus* bus, QObject* cfg)
{
    Q_UNUSED(bus)
    Q_UNUSED(cfg)
    m_videoContainer = nullptr;
    m_allDataWatcher = nullptr;

    EventBus::instance()->subscribe(TOPIC_VIDEO_STREAM_REQUEST, [this](const QString& topic, EventDataPtr data) {
        onStreamRequested(topic, data);
    });
    EventBus::instance()->subscribe(TOPIC_VIDEO_STREAM_STOP, [this](const QString& topic, EventDataPtr data) {
        onStreamStop(topic, data);
    });
    m_sessionLoggedInToken = EventBus::instance()->subscribe(TOPIC_SESSION_LOGGED_IN, [this](const QString& topic, EventDataPtr data) {
        onLoggedIn(topic, data);
    });
    LOG_INFO("LiveVideoPlugin", "Initialized");
    return true;
}

void LiveVideoPlugin::shutdown()
{
    EventBus::instance()->unsubscribeAll();
}

void LiveVideoPlugin::onLoggedIn(const QString& topic, EventDataPtr)
{
    Q_UNUSED(topic)
    LOG_INFO("LiveVideoPlugin", "onLoggedIn, checking data...");
    if (m_videoContainer) {
        m_videoContainer->initialize();
    }

    if (DataManager::instance()->isLoaded()) {
        LOG_INFO("LiveVideoPlugin", "Using DataManager data");
        onDataManagerLoaded();
    } else {
        LOG_INFO("LiveVideoPlugin", "DataManager not loaded yet, loading...");
        connect(DataManager::instance(), &DataManager::loadFinished, this, &LiveVideoPlugin::onDataManagerLoaded, Qt::UniqueConnection);
        DataManager::instance()->loadAll();
    }
}

void LiveVideoPlugin::onDataManagerLoaded()
{
    disconnect(DataManager::instance(), &DataManager::loadFinished, this, &LiveVideoPlugin::onDataManagerLoaded);

    if (!DataManager::instance()->isLoaded()) {
        LOG_ERROR("LiveVideoPlugin", "DataManager load failed");
        return;
    }

    m_regionTree->setNodes(DataManager::instance()->regionTree());
    m_groupTree->setNodes(DataManager::instance()->groupTree());

    LOG_INFO("LiveVideoPlugin", QString("Using DataManager: regionNodes=%1, groupNodes=%2")
        .arg(DataManager::instance()->regionTree().size())
        .arg(DataManager::instance()->groupTree().size()));
}

void LiveVideoPlugin::loadAllDeviceData()
{
    m_regionsFuture = QtConcurrent::run([]() {
        QJsonObject response;
        QString error;
        bool success = ServerApi::instance()->getSync("/api/v1/regions/tree", response, error);
        return QPair<bool, QJsonObject>(success, response);
    });

    m_groupsFuture = QtConcurrent::run([]() {
        QJsonObject response;
        QString error;
        bool success = ServerApi::instance()->getSync("/api/v1/groups/tree", response, error);
        return QPair<bool, QJsonObject>(success, response);
    });

    m_devicesFuture = QtConcurrent::run([]() {
        QJsonObject response;
        QString error;
        bool success = ServerApi::instance()->getSync("/api/v1/devices?limit=500", response, error);
        return QPair<bool, QJsonObject>(success, response);
    });

    m_channelsFuture = QtConcurrent::run([]() {
        QJsonObject response;
        QString error;
        bool success = ServerApi::instance()->getSync("/api/v1/channels?limit=500", response, error);
        return QPair<bool, QJsonObject>(success, response);
    });

    m_allDataWatcher = new QFutureWatcher<QPair<bool, QJsonObject>>(this);
    QObject::connect(m_allDataWatcher, &QFutureWatcher<QPair<bool, QJsonObject>>::finished,
                    this, &LiveVideoPlugin::onAllDataLoaded);

    QFuture<QPair<bool, QJsonObject>> combinedFuture = QtConcurrent::run([this]() {
        m_regionsFuture.waitForFinished();
        m_groupsFuture.waitForFinished();
        m_devicesFuture.waitForFinished();
        m_channelsFuture.waitForFinished();
        return QPair<bool, QJsonObject>(true, QJsonObject());
    });
    m_allDataWatcher->setFuture(combinedFuture);
}

void LiveVideoPlugin::onAllDataLoaded()
{
    QPair<bool, QJsonObject> regionsResult = m_regionsFuture.result();
    QPair<bool, QJsonObject> groupsResult = m_groupsFuture.result();
    QPair<bool, QJsonObject> devicesResult = m_devicesFuture.result();
    QPair<bool, QJsonObject> channelsResult = m_channelsFuture.result();

    if (!regionsResult.first || !groupsResult.first || !devicesResult.first || !channelsResult.first) {
        LOG_ERROR("LiveVideoPlugin", "Failed to load some data");
        return;
    }

    QJsonArray regionsData = regionsResult.second.value("data").toArray();
    QJsonArray groupsData = groupsResult.second.value("data").toArray();
    QJsonArray devicesData = devicesResult.second.value("data").toObject().value("items").toArray();
    QJsonArray channelsData = channelsResult.second.value("data").toObject().value("items").toArray();

    LOG_INFO("LiveVideoPlugin", QString("Loaded: regions=%1 groups=%2 devices=%3 channels=%4")
        .arg(regionsData.size()).arg(groupsData.size()).arg(devicesData.size()).arg(channelsData.size()));

    buildDeviceTrees(regionsData, groupsData, devicesData, channelsData);

    m_allDataWatcher->deleteLater();
    m_allDataWatcher = nullptr;
}

void LiveVideoPlugin::buildDeviceTrees(const QJsonArray& regionsData, const QJsonArray& groupsData,
                                      const QJsonArray& devicesData, const QJsonArray& channelsData)
{
    QList<DeviceTreeNode::Ptr> regionNodes;
    QList<DeviceTreeNode::Ptr> groupNodes;

    QMap<QString, QList<DeviceTreeNode::Ptr>> regionDeviceMap;
    QMap<QString, QList<DeviceTreeNode::Ptr>> groupDeviceMap;

    QMap<QString, QList<DeviceTreeNode::Ptr>> deviceChannelMap;
    std::unordered_map<QString, DeviceTreeNode::Ptr> allDeviceMap;
    for (const QJsonValue& v : devicesData) {
        QJsonObject obj = v.toObject();
        DeviceTreeNode::Ptr deviceNode= std::make_shared<DeviceTreeNode>();
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

    for (const QJsonValue& v : channelsData) {
        QJsonObject obj = v.toObject();
        DeviceTreeNode::Ptr channelNode =std::make_shared<DeviceTreeNode>();
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

        std::unordered_map<QString, DeviceTreeNode::Ptr>::iterator it = allDeviceMap.find(deviceTag);
        if (it != allDeviceMap.end()) {
            it->second->children.append(channelNode);
        }
    }

    for (const QJsonValue& v : regionsData) {
        auto node = parseRegionNode(v.toObject(), regionDeviceMap);
        regionNodes.append(node);
    }

    for (const QJsonValue& v : groupsData) {
        groupNodes.append(parseGroupNode(v.toObject(), groupDeviceMap));
    }

    for (auto& d: allDeviceMap) {
        if (!d.second->inGroup) {
            groupNodes.append(d.second);
        }
        if (!d.second->inRegion) {
            regionNodes.append(d.second);
        }
    }
    m_regionTree->setNodes(regionNodes);
    m_groupTree->setNodes(groupNodes);
}

DeviceTreeNode::Ptr LiveVideoPlugin::parseRegionNode(const QJsonObject& obj,
    const QMap<QString, QList<DeviceTreeNode::Ptr>>& regionDeviceMap)
{
    DeviceTreeNode::Ptr node = std::make_shared<DeviceTreeNode>();;
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

DeviceTreeNode::Ptr LiveVideoPlugin::parseGroupNode(const QJsonObject& obj, const QMap<QString, QList<DeviceTreeNode::Ptr>>& groupDeviceMap)
{
    DeviceTreeNode::Ptr node = std::make_shared<DeviceTreeNode>();;
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

QWidget* LiveVideoPlugin::createWorkspace(QWidget* parent)
{
    LOG_INFO("LiveVideoPlugin", "createWorkspace called");
    QWidget* mainWidget = new QWidget(parent);
    mainWidget->setObjectName("LiveVideoWorkspace");

    QHBoxLayout* mainLayout = new QHBoxLayout(mainWidget);
    mainLayout->setContentsMargins(0, 0, 0, 0);
    mainLayout->setSpacing(4);

    m_sidePanel = new QWidget(mainWidget);
    m_sidePanel->setObjectName("sidePanel");
    m_sidePanel->setMinimumWidth(280);
    m_sidePanel->setMaximumWidth(400);
    QVBoxLayout* sideLayout = new QVBoxLayout(m_sidePanel);
    sideLayout->setContentsMargins(4, 4, 4, 4);
    sideLayout->setSpacing(4);

    m_tabWidget = new QTabWidget(m_sidePanel);
    m_tabWidget->setStyleSheet(R"(
        QTabWidget::pane {
            border: 1px solid #3e3e3e;
            background-color: #2a2a2a;
        }
    )");

    m_regionTree = new DeviceTree(m_tabWidget);
    QObject::connect(m_regionTree, QOverload<QVariantMap>::of(&DeviceTree::itemDoubleClicked),
                     this, &LiveVideoPlugin::onDeviceTreeItemClicked);
    m_tabWidget->addTab(m_regionTree, tr("Region"));

    m_groupTree = new DeviceTree(m_tabWidget);
    QObject::connect(m_groupTree, QOverload<QVariantMap>::of(&DeviceTree::itemDoubleClicked),
                     this, &LiveVideoPlugin::onDeviceTreeItemClicked);
    m_tabWidget->addTab(m_groupTree, tr("Group"));

    sideLayout->addWidget(m_tabWidget, 1);

    m_ptzPanel = new PTZControlPanel(m_sidePanel);
    m_ptzPanel->setSizePolicy(QSizePolicy::Preferred, QSizePolicy::Fixed);
    m_ptzPanel->setMinimumHeight(180);
    m_ptzPanel->setMinimumWidth(200);
    connect(m_ptzPanel, &PTZControlPanel::ptzCommand, this, &LiveVideoPlugin::onPtzCommand);
    sideLayout->addWidget(m_ptzPanel);

    m_videoContainer = new VideoPlayerContainer(mainWidget);

    mainLayout->addWidget(m_sidePanel, 0);
    mainLayout->addWidget(m_videoContainer, 1);

    LOG_INFO("LiveVideoPlugin", "createWorkspace done, returning mainWidget");
    return mainWidget;
}

QWidget* LiveVideoPlugin::createSidePanel(QWidget* parent)
{
    m_sidePanel = new QWidget(parent);
    m_sidePanel->setObjectName("VideoSidePanel");

    QVBoxLayout* layout = new QVBoxLayout(m_sidePanel);
    layout->setContentsMargins(0, 0, 0, 0);

    m_tabWidget = new QTabWidget(m_sidePanel);

    m_regionTree = new DeviceTree(m_tabWidget);
    QObject::connect(m_regionTree, QOverload<QVariantMap>::of(&DeviceTree::itemDoubleClicked),
                     this, &LiveVideoPlugin::onDeviceTreeItemClicked);
    m_tabWidget->addTab(m_regionTree, tr("Region"));

    m_groupTree = new DeviceTree(m_tabWidget);
    QObject::connect(m_groupTree, QOverload<QVariantMap>::of(&DeviceTree::itemDoubleClicked),
                     this, &LiveVideoPlugin::onDeviceTreeItemClicked);
    m_tabWidget->addTab(m_groupTree, tr("Group"));

    layout->addWidget(m_tabWidget);

    m_ptzPanel = new PTZControlPanel(m_sidePanel);
    connect(m_ptzPanel, &PTZControlPanel::ptzCommand, this, &LiveVideoPlugin::onPtzCommand);
    layout->addWidget(m_ptzPanel);

    return m_sidePanel;
}

void LiveVideoPlugin::onStreamRequested(const QString& topic, EventDataPtr)
{
    Q_UNUSED(topic)
}

void LiveVideoPlugin::onStreamStop(const QString& topic, EventDataPtr)
{
    Q_UNUSED(topic)
}

void LiveVideoPlugin::onDeviceTreeItemClicked(const QVariantMap& meta)
{
    if (!m_videoContainer) {
        LOG_ERROR("LiveVideoPlugin", "m_videoContainer is null!");
        return;
    }

    const QString type = meta["type"].toString();
    const QString deviceTag = meta["device_tag"].toString();
    if (deviceTag.isEmpty()) {
        return;
    }

    // 设备级播放也交给后端生成地址（以 device_tag 作为默认通道）
    QString channelTag = deviceTag;
    if (type == "channel") {
        channelTag = meta["channel_tag"].toString();
        if (channelTag.isEmpty()) {
            return;
        }
    } else if (type != "device") {
        return;
    }

    m_videoContainer->playChannel(deviceTag, channelTag, meta["name"].toString());
}

void LiveVideoPlugin::onPtzCommand(const QString& command, const QVariant& value) {
    if (!m_ptzPanel) {
        LOG_WARNING("LiveVideoPlugin", "No PTZ panel");
        return;
    }

    // PTZ 控制的是面板当前的目标通道（画面格子 id 和通道身份是两回事）
    const QString deviceTag = m_ptzPanel->deviceId();
    const QString channelTag = m_ptzPanel->channelId();
    if (deviceTag.isEmpty() || channelTag.isEmpty()) {
        LOG_WARNING("LiveVideoPlugin", "No PTZ target channel selected");
        return;
    }

    LOG_INFO("LiveVideoPlugin", QString("PTZ command: %1, value: %2, device: %3, channel: %4").arg(command).arg(value.toString()).arg(deviceTag).arg(channelTag));

    QString apiPath;
    if (command == "direction") {
        apiPath = QString("/api/v1/channels/%1/%2/ptz/direction?dir=%3").arg(deviceTag, channelTag, value.toString());
        ServerApi::instance()->get(apiPath, [this](bool success, const QJsonObject& resp, const QString& error) {
            if (!success) {
                LOG_ERROR("LiveVideoPlugin", QString("PTZ command failed: %1").arg(error));
            }
        });
    } else if (command == "center") {
        apiPath = QString("/api/v1/channels/%1/%2/ptz/center").arg(deviceTag, channelTag);
        ServerApi::instance()->get(apiPath, [this](bool success, const QJsonObject& resp, const QString& error) {
            if (!success) {
                LOG_ERROR("LiveVideoPlugin", QString("PTZ command failed: %1").arg(error));
            }
        });
    } else if (command == "aux") {
        apiPath = QString("/api/v1/channels/%1/%2/ptz/aux?func=%3").arg(deviceTag, channelTag, value.toString());
        ServerApi::instance()->get(apiPath, [this](bool success, const QJsonObject& resp, const QString& error) {
            if (!success) {
                LOG_ERROR("LiveVideoPlugin", QString("PTZ command failed: %1").arg(error));
            }
        });
    } else if (command == "add_preset") {
        apiPath = QString("/api/v1/channels/%1/%2/ptz/presets").arg(deviceTag, channelTag);
        QJsonObject body;
        body["name"] = value.toString();
        ServerApi::instance()->post(apiPath, body, [this](bool success, const QJsonObject& resp, const QString& error) {
            if (!success) {
                LOG_ERROR("LiveVideoPlugin", QString("Add preset failed: %1").arg(error));
            } else {
                LOG_INFO("LiveVideoPlugin", "Preset added successfully");
                m_ptzPanel->reloadPresets();
            }
        });
    } else if (command == "delete_preset") {
        apiPath = QString("/api/v1/channels/%1/%2/ptz/presets/%3").arg(deviceTag, channelTag, value.toString());
        ServerApi::instance()->deleteResource(apiPath, [this](bool success, const QJsonObject& resp, const QString& error) {
            if (!success) {
                LOG_ERROR("LiveVideoPlugin", QString("Delete preset failed: %1").arg(error));
            } else {
                LOG_INFO("LiveVideoPlugin", "Preset deleted successfully");
                m_ptzPanel->reloadPresets();
            }
        });
    } else if (command == "preset") {
        apiPath = QString("/api/v1/channels/%1/%2/ptz/presets/%3/goto").arg(deviceTag, channelTag, value.toString());
        ServerApi::instance()->get(apiPath, [this](bool success, const QJsonObject& resp, const QString& error) {
            if (!success) {
                LOG_ERROR("LiveVideoPlugin", QString("Goto preset failed: %1").arg(error));
            }
        });
    } else {
        LOG_WARNING("LiveVideoPlugin", QString("Unknown PTZ command: %1").arg(command));
    }
}

}
