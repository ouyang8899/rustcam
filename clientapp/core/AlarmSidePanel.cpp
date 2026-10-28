#include "AlarmSidePanel.h"
#include "DataManager.h"
#include "DeviceTreeModel.h"
#include "core/ServerApi.h"
#include "common/Logger.h"
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QComboBox>
#include <QTableView>
#include <QHeaderView>
#include <QStandardItemModel>
#include <QDateTime>

namespace rustcam {

AlarmSidePanel::AlarmSidePanel(QWidget* parent)
    : QWidget(parent)
    , m_regionTree(nullptr)
    , m_groupTree(nullptr)
    , m_deviceTree(nullptr)
    , m_alarmModel(nullptr)
    , m_alarmTable(nullptr)
{
    QHBoxLayout* mainLayout = new QHBoxLayout(this);
    mainLayout->setContentsMargins(8, 8, 8, 8);
    mainLayout->setSpacing(8);

    m_tabWidget = new QTabWidget(this);
    m_tabWidget->setMinimumWidth(200);
    m_tabWidget->setMaximumWidth(300);

    m_regionTree = new DeviceTree(m_tabWidget);
    m_groupTree = new DeviceTree(m_tabWidget);
    m_deviceTree = new DeviceTree(m_tabWidget);

    m_tabWidget->addTab(m_regionTree, tr("Region"));
    m_tabWidget->addTab(m_groupTree, tr("Group"));
    m_tabWidget->addTab(m_deviceTree, tr("Device"));

    mainLayout->addWidget(m_tabWidget);

    QWidget* rightPanel = new QWidget(this);
    QVBoxLayout* rightLayout = new QVBoxLayout(rightPanel);
    rightLayout->setContentsMargins(0, 0, 0, 0);
    rightLayout->setSpacing(8);

    QLabel* alarmLabel = new QLabel(tr("Alarm List"), rightPanel);
    alarmLabel->setObjectName("sectionTitle");
    rightLayout->addWidget(alarmLabel);

    QFrame* alarmFrame = new QFrame(rightPanel);
    alarmFrame->setObjectName("cardFrame");
    QVBoxLayout* alarmLayout = new QVBoxLayout(alarmFrame);
    alarmLayout->setContentsMargins(8, 8, 8, 8);
    alarmLayout->setSpacing(4);

    m_alarmModel = new QStandardItemModel(this);
    m_alarmModel->setHorizontalHeaderLabels({tr("Time"), tr("Device"), tr("Type"), tr("Level"), tr("Status")});

    m_alarmTable = new QTableView(alarmFrame);
    m_alarmTable->setModel(m_alarmModel);
    m_alarmTable->setAlternatingRowColors(true);
    m_alarmTable->setSelectionBehavior(QAbstractItemView::SelectRows);
    m_alarmTable->verticalHeader()->hide();
    m_alarmTable->horizontalHeader()->setSectionResizeMode(QHeaderView::Stretch);
    connect(m_alarmTable, &QTableView::clicked, this, &AlarmSidePanel::onAlarmClicked);
    alarmLayout->addWidget(m_alarmTable, 1);

    QHBoxLayout* pagingLayout = new QHBoxLayout();
    pagingLayout->setSpacing(8);

    m_prevBtn = new QPushButton(tr("Previous Page"), alarmFrame);
    connect(m_prevBtn, &QPushButton::clicked, this, &AlarmSidePanel::onPrevPage);
    pagingLayout->addWidget(m_prevBtn);

    m_pageLabel = new QLabel(tr("Page 1 / 1 (Total 0)"), alarmFrame);
    m_pageLabel->setObjectName("secondaryText");
    pagingLayout->addWidget(m_pageLabel);
    pagingLayout->addStretch();

    m_pageSizeCombo = new QComboBox(alarmFrame);
    m_pageSizeCombo->addItems({"10", "20", "50", "100"});
    m_pageSizeCombo->setCurrentText("20");
    connect(m_pageSizeCombo, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, &AlarmSidePanel::onPageSizeChanged);
    pagingLayout->addWidget(m_pageSizeCombo);

    m_nextBtn = new QPushButton(tr("Next Page"), alarmFrame);
    connect(m_nextBtn, &QPushButton::clicked, this, &AlarmSidePanel::onNextPage);
    pagingLayout->addWidget(m_nextBtn);

    alarmLayout->addLayout(pagingLayout);
    rightLayout->addWidget(alarmFrame, 1);
    mainLayout->addWidget(rightPanel, 1);

    connect(m_regionTree, &DeviceTree::itemDoubleClicked, this, &AlarmSidePanel::onDeviceItemDoubleClicked);
    connect(m_groupTree, &DeviceTree::itemDoubleClicked, this, &AlarmSidePanel::onDeviceItemDoubleClicked);
    connect(m_deviceTree, &DeviceTree::itemDoubleClicked, this, &AlarmSidePanel::onDeviceItemDoubleClicked);

    loadAlarms();
}

AlarmSidePanel::~AlarmSidePanel() = default;

void AlarmSidePanel::setRegionTreeNodes(const QList<DeviceTreeNode::Ptr>& nodes) {
    m_regionTree->setNodes(nodes);
}

void AlarmSidePanel::setGroupTreeNodes(const QList<DeviceTreeNode::Ptr>& nodes) {
    m_groupTree->setNodes(nodes);
}

void AlarmSidePanel::setDeviceTreeNodes(const QList<DeviceTreeNode::Ptr>& nodes) {
    m_deviceTree->setNodes(nodes);
}

void AlarmSidePanel::loadAlarms(int page, int pageSize) {
    m_currentPage = page;
    m_pageSize = pageSize;

    QString endpoint = QString("/api/v1/alarms?page=%1&page_size=%2").arg(page).arg(pageSize);
    if (!m_currentDeviceId.isEmpty()) {
        endpoint += QString("&device_id=%1").arg(m_currentDeviceId);
    }
    if (!m_filter.isEmpty()) {
        endpoint += QString("&filter=%1").arg(m_filter);
    }

    ServerApi::instance()->get(endpoint, [this](bool success, const QJsonObject& resp, const QString& error) {
        if (!success) {
            LOG_ERROR("AlarmSidePanel", QString("Failed to load alarms: %1").arg(error));
            return;
        }

        QJsonObject data = resp["data"].toObject();
        m_totalCount = data["total"].toInt();
        const int pageSize = data["page_size"].toInt(m_pageSize > 0 ? m_pageSize : 20);
        m_totalPages = pageSize > 0 ? (m_totalCount + pageSize - 1) / pageSize : 1;
        if (m_totalPages < 1) m_totalPages = 1;

        QJsonArray alarms = data["items"].toArray();
        m_alarmModel->removeRows(0, m_alarmModel->rowCount());

        const QJsonArray devices = DataManager::instance()->devicesData();
        auto resolveDevice = [&devices](const QString& tag) {
            for (const QJsonValue& v : devices) {
                const QJsonObject device = v.toObject();
                if (device.value("device_tag").toString() == tag) {
                    const QString name = device.value("name").toString();
                    if (!name.isEmpty()) return name;
                }
            }
            return tag;
        };
        auto formatTime = [](const QString& iso) {
            QDateTime dt = QDateTime::fromString(iso, Qt::ISODate);
            return dt.isValid() ? dt.toLocalTime().toString("yyyy-MM-dd HH:mm:ss") : iso;
        };
        auto levelText = [this](int priority) {
            switch (priority) {
            case 1: return tr("Critical");
            case 2: return tr("Major");
            case 3: return tr("Warning");
            case 4: return tr("Info");
            default: return tr("Normal");
            }
        };

        for (const QJsonValue& v : alarms) {
            const QJsonObject alarm = v.toObject();
            const QString time = formatTime(alarm["alarm_time"].toString());
            const QString deviceTag = alarm["device_tag"].toString();
            const QString device = resolveDevice(deviceTag);
            const QString type = alarm["alarm_type"].toString();
            const int priority = alarm["alarm_priority"].toInt();
            const QString level = levelText(priority);
            const bool processed = alarm["processed"].toBool();
            const QString status = processed ? tr("Processed") : tr("Unprocessed");
            const QString alarmId = QString::number(alarm["id"].toVariant().toLongLong());

            QList<QStandardItem*> row;
            row.append(new QStandardItem(time));
            row.append(new QStandardItem(device));
            row.append(new QStandardItem(type));
            row.append(new QStandardItem(level));
            row.append(new QStandardItem(status));

            for (auto item : row) {
                item->setData(alarmId, Qt::UserRole);
                if (!processed && priority <= 2) {
                    item->setForeground(QColor(priority <= 1 ? "#f44336" : "#ff9800"));
                }
            }

            m_alarmModel->appendRow(row);
        }

        m_pageLabel->setText(tr("Page %1 / %2 (Total %3)")
            .arg(m_currentPage).arg(m_totalPages).arg(m_totalCount));
        m_prevBtn->setEnabled(m_currentPage > 1);
        m_nextBtn->setEnabled(m_currentPage < m_totalPages);

        LOG_INFO("AlarmSidePanel", QString("Loaded %1/%2 alarms").arg(alarms.size()).arg(m_totalCount));
    });
}

void AlarmSidePanel::onDeviceItemDoubleClicked(QVariantMap meta) {
    QString type = meta["type"].toString();
    if (type == "channel" || type == "device") {
        QString deviceTag = meta["device_tag"].toString();
        QString deviceName = meta["name"].toString();
        m_currentDeviceId = deviceTag;
        emit deviceSelected(deviceTag, deviceName);
        loadAlarms(1, m_pageSize);
    }
}

void AlarmSidePanel::onAlarmClicked(const QModelIndex& index) {
    if (!index.isValid()) return;
    QString alarmId = index.data(Qt::UserRole).toString();
    if (!alarmId.isEmpty()) {
        emit alarmSelected(alarmId);
    }
}

void AlarmSidePanel::onPrevPage() {
    if (m_currentPage > 1) {
        loadAlarms(m_currentPage - 1, m_pageSize);
        emit pageChanged(m_currentPage - 1);
    }
}

void AlarmSidePanel::onNextPage() {
    if (m_currentPage < m_totalPages) {
        loadAlarms(m_currentPage + 1, m_pageSize);
        emit pageChanged(m_currentPage + 1);
    }
}

void AlarmSidePanel::onPageSizeChanged(int index) {
    Q_UNUSED(index)
    int size = m_pageSizeCombo->currentText().toInt();
    if (size != m_pageSize) {
        m_pageSize = size;
        loadAlarms(1, m_pageSize);
    }
}

void AlarmSidePanel::onAlarmReply(bool success, const QJsonObject& resp, const QString& error) {
    Q_UNUSED(resp)
    if (success) {
        LOG_INFO("AlarmSidePanel", "Alarm acknowledged");
        loadAlarms(m_currentPage, m_pageSize);
    } else {
        LOG_ERROR("AlarmSidePanel", QString("Failed to acknowledge alarm: %1").arg(error));
    }
}

}
