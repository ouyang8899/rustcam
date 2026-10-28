#include "AlarmPlugin.h"
#include "core/EventBus.h"
#include "core/ServerApi.h"
#include "common/Logger.h"
#include <QWidget>
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QPushButton>
#include <QLabel>
#include <QComboBox>
#include <QToolBar>
#include <QDateTime>

namespace rustcam {

bool AlarmPlugin::initialize(EventBus* bus, QObject* cfg)
{
    Q_UNUSED(bus)
    Q_UNUSED(cfg)

    m_unreadCount = 0;

    EventBus::instance()->subscribe("alarm/triggered", [this](const QString& topic, EventDataPtr data) {
        Q_UNUSED(topic)
        QVariantMap payload = data->m_payload;
        QString alarmId = payload["alarmId"].toString();
        QString deviceName = payload["deviceName"].toString();
        QString type = payload["type"].toString();
        QString level = payload["level"].toString();

        LOG_INFO("AlarmPlugin", QString("Alarm triggered: %1 from %2").arg(alarmId).arg(deviceName));

        m_unreadCount++;
        emit alarmCountChanged(m_unreadCount);

        if (m_sidePanel) {
            m_sidePanel->loadAlarms();
        }
    });

    if (DataManager::instance()->isLoaded()) {
        onDataManagerLoaded();
    } else {
        connect(DataManager::instance(), &DataManager::loadFinished, this, &AlarmPlugin::onDataManagerLoaded, Qt::UniqueConnection);
    }

    return true;
}

void AlarmPlugin::shutdown()
{
    EventBus::instance()->unsubscribeAll();
}

QWidget* AlarmPlugin::createWorkspace(QWidget* parent)
{
    m_workspace = new QWidget(parent);
    m_workspace->setObjectName("workspaceArea");

    QVBoxLayout* layout = new QVBoxLayout(m_workspace);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    QToolBar* toolbar = new QToolBar(m_workspace);

    QLabel* titleLabel = new QLabel(tr("Alarm Center"), toolbar);
    titleLabel->setObjectName("sectionTitle");
    toolbar->addWidget(titleLabel);

    toolbar->addSeparator();

    QPushButton* refreshBtn = new QPushButton(tr("Refresh"), toolbar);
    refreshBtn->setObjectName("primaryButton");
    refreshBtn->setCursor(Qt::PointingHandCursor);
    connect(refreshBtn, &QPushButton::clicked, this, &AlarmPlugin::onRefreshClicked);
    toolbar->addWidget(refreshBtn);

    QLabel* countLabel = new QLabel(m_workspace);
    countLabel->setObjectName("alarmCount");
    m_countLabel = countLabel;
    connect(this, &AlarmPlugin::alarmCountChanged, this, &AlarmPlugin::onAlarmCountChanged);
    toolbar->addWidget(countLabel);

    toolbar->addSeparator();

    QComboBox* filterCombo = new QComboBox(toolbar);
    filterCombo->addItems({tr("All"), tr("Unprocessed"), tr("Processed"), tr("Alarm"), tr("Fault")});
    connect(filterCombo, &QComboBox::currentTextChanged, this, &AlarmPlugin::onFilterChanged);
    toolbar->addWidget(filterCombo);

    layout->addWidget(toolbar);

    m_sidePanel = new AlarmSidePanel(m_workspace);
    connect(m_sidePanel, &AlarmSidePanel::deviceSelected, this, &AlarmPlugin::onDeviceSelected);
    connect(m_sidePanel, &AlarmSidePanel::alarmSelected, this, &AlarmPlugin::onAlarmSelected);

    layout->addWidget(m_sidePanel, 1);

    return m_workspace;
}

QWidget* AlarmPlugin::createSidePanel(QWidget* parent)
{
    m_sidePanel = new AlarmSidePanel(parent);
    connect(m_sidePanel, &AlarmSidePanel::deviceSelected, this, &AlarmPlugin::onDeviceSelected);
    connect(m_sidePanel, &AlarmSidePanel::alarmSelected, this, &AlarmPlugin::onAlarmSelected);
    return m_sidePanel;
}

void AlarmPlugin::onDataManagerLoaded() {
    disconnect(DataManager::instance(), &DataManager::loadFinished, this, &AlarmPlugin::onDataManagerLoaded);

    if (m_sidePanel) {
        m_sidePanel->setRegionTreeNodes(DataManager::instance()->regionTree());
        m_sidePanel->setGroupTreeNodes(DataManager::instance()->groupTree());
        m_sidePanel->setDeviceTreeNodes(DataManager::instance()->deviceList());
    }

    LOG_INFO("AlarmPlugin", QString("Using DataManager: regionNodes=%1, groupNodes=%2, deviceNodes=%3")
        .arg(DataManager::instance()->regionTree().size())
        .arg(DataManager::instance()->groupTree().size())
        .arg(DataManager::instance()->deviceList().size()));
}

void AlarmPlugin::loadAlarms()
{
    if (m_sidePanel) {
        m_sidePanel->loadAlarms();
    }
}

void AlarmPlugin::onRefreshClicked()
{
    loadAlarms();
}

void AlarmPlugin::onFilterChanged(const QString& text)
{
    if (!m_sidePanel) {
        return;
    }
    QString filter;
    if (text == tr("Unprocessed")) filter = "unhandled";
    else if (text == tr("Processed")) filter = "handled";
    else if (text == tr("Alarm")) filter = "alarm";
    else if (text == tr("Fault")) filter = "fault";
    m_sidePanel->setFilter(filter);
    m_sidePanel->loadAlarms();
}

void AlarmPlugin::onAlarmCountChanged(int count)
{
    if (m_countLabel) {
        m_countLabel->setText(tr("Unprocessed: %1").arg(count));
    }
}

void AlarmPlugin::addAlarmItem(const QJsonObject& alarm)
{
    Q_UNUSED(alarm)
}

void AlarmPlugin::onAlarmTriggered(const QString& topic, EventDataPtr data)
{
    Q_UNUSED(topic)

    QVariantMap payload = data->m_payload;
    QString alarmId = payload["alarmId"].toString();
    QString deviceName = payload["deviceName"].toString();
    QString type = payload["type"].toString();
    QString level = payload["level"].toString();

    LOG_INFO("AlarmPlugin", QString("Alarm triggered: %1 from %2").arg(alarmId).arg(deviceName));

    m_unreadCount++;
    emit alarmCountChanged(m_unreadCount);

    if (m_sidePanel) {
        m_sidePanel->loadAlarms();
    }
}

void AlarmPlugin::onDeviceSelected(const QString& deviceId, const QString& deviceName)
{
    Q_UNUSED(deviceId)
    Q_UNUSED(deviceName)
}

void AlarmPlugin::onAlarmSelected(const QString& alarmId)
{
    QJsonObject payload;
    payload["alarmId"] = alarmId;

    ServerApi::instance()->post("/api/v1/alarms/acknowledge", payload, [this](bool success, const QJsonObject&, const QString& error) {
        if (success) {
            m_unreadCount = qMax(0, m_unreadCount - 1);
            emit alarmCountChanged(m_unreadCount);
            if (m_sidePanel) {
                m_sidePanel->loadAlarms();
            }
        } else {
            LOG_ERROR("AlarmPlugin", QString("Failed to acknowledge alarm: %1").arg(error));
        }
    });
}

}
