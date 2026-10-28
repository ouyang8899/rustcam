#include "PlaybackSidePanel.h"
#include "PlaybackChannel.h"
#include "common/Logger.h"
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QComboBox>
#include <QStackedWidget>
#include <QLabel>
#include <QPushButton>
#include <QMap>

namespace rustcam {

PlaybackSidePanel::PlaybackSidePanel(QWidget* parent)
    : QWidget(parent)
{
    setObjectName("PlaybackSidePanel");
    setAttribute(Qt::WA_StyledBackground, true);
    setMinimumWidth(280);
    setMaximumWidth(350);

    QVBoxLayout* mainLayout = new QVBoxLayout(this);
    mainLayout->setContentsMargins(4, 4, 4, 4);
    mainLayout->setSpacing(4);

    QLabel* deviceLabel = new QLabel(tr("Device List"), this);
    deviceLabel->setObjectName("PlaybackSectionTitle");
    mainLayout->addWidget(deviceLabel);

    m_tabWidget = new QTabWidget(this);

    m_regionTree = new DeviceTree(m_tabWidget);
    m_groupTree = new DeviceTree(m_tabWidget);
    m_deviceTree = new DeviceTree(m_tabWidget);

    m_tabWidget->addTab(m_regionTree, tr("Region"));
    m_tabWidget->addTab(m_groupTree, tr("Group"));
    m_tabWidget->addTab(m_deviceTree, tr("Device"));

    mainLayout->addWidget(m_tabWidget, 1);

    // 通道切换：视频窗口不变，只切换侧栏与时间轴
    QHBoxLayout* switcherLayout = new QHBoxLayout();
    switcherLayout->setSpacing(4);
    QLabel* switcherLabel = new QLabel(tr("查看通道:"), this);
    switcherLabel->setObjectName("PlaybackPanelLabel");
    switcherLayout->addWidget(switcherLabel);

    m_channelCombo = new QComboBox(this);
    m_channelCombo->setToolTip(tr("切换正在查看录像的通道"));
    connect(m_channelCombo, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, &PlaybackSidePanel::onChannelComboChanged);
    switcherLayout->addWidget(m_channelCombo, 1);

    QPushButton* closeBtn = new QPushButton(tr("关闭"), this);
    closeBtn->setFixedWidth(48);
    closeBtn->setToolTip(tr("从列表中移除当前通道（播放窗口解绑）"));
    connect(closeBtn, &QPushButton::clicked, this, &PlaybackSidePanel::onCloseChannelClicked);
    switcherLayout->addWidget(closeBtn);
    mainLayout->addLayout(switcherLayout);

    m_channelStack = new QStackedWidget(this);
    mainLayout->addWidget(m_channelStack, 1);

    connect(m_regionTree, &DeviceTree::itemDoubleClicked, this, &PlaybackSidePanel::onDeviceItemDoubleClicked);
    connect(m_groupTree, &DeviceTree::itemDoubleClicked, this, &PlaybackSidePanel::onDeviceItemDoubleClicked);
    connect(m_deviceTree, &DeviceTree::itemDoubleClicked, this, &PlaybackSidePanel::onDeviceItemDoubleClicked);
}

PlaybackSidePanel::~PlaybackSidePanel() {
}

// ── 通道管理 ────────────────────────────────────────────────────────

PlaybackChannel* PlaybackSidePanel::registerChannel(const QString& deviceTag,
                                                    const QString& channelTag,
                                                    const QString& displayName) {
    const QString key = deviceTag + "/" + channelTag;
    if (m_channels.contains(key)) {
        setActiveChannel(key);
        return m_channels.value(key);
    }

    PlaybackChannel* channel = new PlaybackChannel(deviceTag, channelTag, displayName, this);
    // 通道由侧栏拥有：连接在这里一次建立，之后不再变动
    connect(channel, &PlaybackChannel::filesLoaded, this, &PlaybackSidePanel::channelFilesLoaded);
    connect(channel, &PlaybackChannel::rangeChanged, this, &PlaybackSidePanel::channelRangeChanged);
    connect(channel, &PlaybackChannel::alarmsLoaded, this, &PlaybackSidePanel::channelAlarmsLoaded);
    connect(channel, &PlaybackChannel::bookmarksChanged, this, &PlaybackSidePanel::channelBookmarksChanged);
    connect(channel, &PlaybackChannel::playFileRequested, this, &PlaybackSidePanel::channelPlayFileRequested);
    addChannelPage(key, channel);
    m_channels.insert(key, channel);

    m_channelCombo->addItem(displayName.isEmpty() ? channelTag : displayName, key);
    setActiveChannel(key);
    return channel;
}

void PlaybackSidePanel::addChannelPage(const QString& key, PlaybackChannel* channel) {
    QWidget* page = channel->createSidePanelPage(m_channelStack);
    m_channelStack->addWidget(page);
    m_channelPages.insert(key, page);
}

void PlaybackSidePanel::unregisterChannel(const QString& key) {
    PlaybackChannel* channel = m_channels.take(key);
    if (!channel) {
        return;
    }
    QWidget* page = m_channelPages.take(key);
    if (page) {
        m_channelStack->removeWidget(page);
        page->deleteLater();
    }
    const int index = m_channelCombo->findData(key);
    if (index >= 0) {
        m_channelCombo->removeItem(index);
    }
    if (m_activeChannelKey == key) {
        m_activeChannelKey.clear();
        if (m_channelCombo->count() > 0) {
            setActiveChannel(m_channelCombo->itemData(0).toString());
        }
    }
    channel->deleteLater();
}

bool PlaybackSidePanel::hasChannel(const QString& key) const {
    return m_channels.contains(key);
}

PlaybackChannel* PlaybackSidePanel::channel(const QString& key) const {
    return m_channels.value(key, nullptr);
}

QList<PlaybackChannel*> PlaybackSidePanel::channels() const {
    return m_channels.values();
}

PlaybackChannel* PlaybackSidePanel::activeChannel() const {
    return m_channels.value(m_activeChannelKey, nullptr);
}

QString PlaybackSidePanel::activeChannelKey() const {
    return m_activeChannelKey;
}

void PlaybackSidePanel::setActiveChannel(const QString& key) {
    if (key.isEmpty() || !m_channels.contains(key)) {
        return;
    }
    m_activeChannelKey = key;
    QWidget* page = m_channelPages.value(key, nullptr);
    if (page) {
        m_channelStack->setCurrentWidget(page);
    }
    const int index = m_channelCombo->findData(key);
    if (index >= 0 && index != m_channelCombo->currentIndex()) {
        m_switchingChannel = true;
        m_channelCombo->setCurrentIndex(index);
        m_switchingChannel = false;
    }
    emit activeChannelChanged(key);
}

// ── 设备树 ──────────────────────────────────────────────────────────

void PlaybackSidePanel::onChannelComboChanged(int index) {
    if (m_switchingChannel || index < 0) {
        return;
    }
    setActiveChannel(m_channelCombo->itemData(index).toString());
}

void PlaybackSidePanel::onCloseChannelClicked() {
    if (!m_activeChannelKey.isEmpty()) {
        emit channelClosed(m_activeChannelKey);
    }
}

void PlaybackSidePanel::onDeviceItemDoubleClicked(QVariantMap meta) {
    const QString type = meta["type"].toString();
    if (type != "channel" && type != "device") {
        return;
    }
    const QString deviceTag = meta["device_tag"].toString();
    const QString channelTag = meta["channel_tag"].toString();
    if (deviceTag.isEmpty() && channelTag.isEmpty()) {
        return;
    }
    emit channelRequested(deviceTag, channelTag, meta["name"].toString());
}

void PlaybackSidePanel::setRegionTreeNodes(const QList<DeviceTreeNode::Ptr>& nodes) {
    m_regionTree->setNodes(nodes);
}

void PlaybackSidePanel::setGroupTreeNodes(const QList<DeviceTreeNode::Ptr>& nodes) {
    m_groupTree->setNodes(nodes);
}

void PlaybackSidePanel::setDeviceTreeNodes(const QList<DeviceTreeNode::Ptr>& nodes) {
    m_deviceTree->setNodes(nodes);
}

}
