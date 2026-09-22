#include "AlarmPlugin.h"
#include "core/EventBus.h"
#include "core/ServerApi.h"
#include "core/Logger.h"
#include <QWidget>
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QTableView>
#include <QHeaderView>
#include <QPushButton>
#include <QLabel>
#include <QComboBox>
#include <QToolBar>
#include <QStandardItemModel>
#include <QDateTime>

namespace rustcam {

class AlarmWidget : public QWidget {
    Q_OBJECT
public:
    explicit AlarmWidget(AlarmPlugin* plugin, QWidget* parent = nullptr)
        : QWidget(parent)
        , m_plugin(plugin)
    {
        setupUi();
    }

private slots:
    void onRefresh() {
        m_plugin->loadAlarms();
    }

    void onFilterChanged(const QString& text) {
        Q_UNUSED(text)
    }

    void onAlarmClicked(const QModelIndex& index) {
        if (!index.isValid()) return;
        QString alarmId = index.data(Qt::UserRole).toString();
        if (!alarmId.isEmpty()) {
            m_plugin->onAcknowledgeAlarm(alarmId);
        }
    }

private:
    void setupUi() {
        QVBoxLayout* layout = new QVBoxLayout(this);
        layout->setContentsMargins(8, 8, 8, 8);
        layout->setSpacing(8);

        QToolBar* toolbar = new QToolBar(this);
        toolbar->setStyleSheet(R"(
            QToolBar {
                background-color: #1e1e1e;
                border: none;
                spacing: 8px;
                padding: 4px;
            }
        )");

        QLabel* titleLabel = new QLabel("告警列表", this);
        titleLabel->setStyleSheet(R"(
            QLabel {
                color: #e0e0e0;
                font-size: 14px;
                font-weight: bold;
            }
        )");
        toolbar->addWidget(titleLabel);

        toolbar->addSeparator();

        QPushButton* refreshBtn = new QPushButton("刷新", this);
        refreshBtn->setCursor(Qt::PointingHandCursor);
        refreshBtn->setStyleSheet(R"(
            QPushButton {
                background-color: #0078D4;
                color: white;
                border: none;
                border-radius: 4px;
                padding: 6px 16px;
            }
            QPushButton:hover {
                background-color: #1084d8;
            }
        )");
        connect(refreshBtn, &QPushButton::clicked, this, &AlarmWidget::onRefresh);
        toolbar->addWidget(refreshBtn);

        layout->addWidget(toolbar);

        QComboBox* filterCombo = new QComboBox(this);
        filterCombo->addItems({"全部", "未处理", "已处理", "告警", "故障"});
        filterCombo->setStyleSheet(R"(
            QComboBox {
                background-color: #2b2b2b;
                border: 1px solid #3e3e3e;
                border-radius: 4px;
                padding: 6px 12px;
                color: #e0e0e0;
            }
        )");
        connect(filterCombo, &QComboBox::currentTextChanged, this, &AlarmWidget::onFilterChanged);
        layout->addWidget(filterCombo);

        m_tableView = new QTableView(this);
        m_tableView->setModel(m_plugin->alarmModel());
        m_tableView->setStyleSheet(R"(
            QTableView {
                background-color: #1a1a1a;
                border: 1px solid #3e3e3e;
                border-radius: 4px;
                color: #e0e0e0;
                gridline-color: #3e3e3e;
            }
            QTableView::item {
                padding: 8px;
            }
            QTableView::item:selected {
                background-color: rgba(0, 120, 212, 0.3);
            }
        )");
        m_tableView->setAlternatingRowColors(true);
        m_tableView->setSelectionBehavior(QAbstractItemView::SelectRows);
        m_tableView->horizontalHeader()->setStyleSheet(R"(
            QHeaderView::section {
                background-color: #1e1e1e;
                color: #e0e0e0;
                padding: 8px;
                border: none;
                border-bottom: 1px solid #3e3e3e;
            }
        )");
        m_tableView->verticalHeader()->hide();
        connect(m_tableView, &QTableView::clicked, this, &AlarmWidget::onAlarmClicked);
        layout->addWidget(m_tableView, 1);
    }

    AlarmPlugin* m_plugin;
    QTableView* m_tableView;
};

bool AlarmPlugin::initialize(EventBus* bus, QObject* cfg)
{
    Q_UNUSED(cfg)
    m_unreadCount = 0;
    m_alarmModel = new QStandardItemModel(this);
    m_alarmModel->setHorizontalHeaderLabels({"时间", "设备", "类型", "级别", "状态"});

    EventBus::instance()->subscribe("alarm/triggered", this, SLOT(onAlarmTriggered(QString,QVariantMap)));

    loadAlarms();

    return true;
}

void AlarmPlugin::shutdown()
{
    EventBus::instance()->unsubscribeAll(this);
}

QWidget* AlarmPlugin::createWorkspace(QWidget* parent)
{
    return new AlarmWidget(this, parent);
}

QWidget* AlarmPlugin::createSidePanel(QWidget* parent)
{
    return new AlarmWidget(this, parent);
}

void AlarmPlugin::loadAlarms()
{
    ServerApi::instance()->get("/api/v1/alarms", [this](bool success, const QJsonObject& resp, const QString&) {
        if (success) {
            QJsonArray alarms = resp["data"].toArray();
            m_alarmModel->clear();
            m_alarmModel->setHorizontalHeaderLabels({"时间", "设备", "类型", "级别", "状态"});
            for (const auto& alarm : alarms) {
                addAlarmItem(alarm.toObject());
            }
            LOG_INFO("AlarmPlugin", QString("Loaded %1 alarms").arg(alarms.size()));
        }
    });
}

void AlarmPlugin::addAlarmItem(const QJsonObject& alarm)
{
    QString time = alarm["time"].toString();
    QString device = alarm["deviceName"].toString();
    QString type = alarm["type"].toString();
    QString level = alarm["level"].toString();
    QString status = alarm["status"].toString();

    QList<QStandardItem*> row;
    row.append(new QStandardItem(time));
    row.append(new QStandardItem(device));
    row.append(new QStandardItem(type));
    row.append(new QStandardItem(level));
    row.append(new QStandardItem(status));

    for (auto item : row) {
        item->setData(alarm["id"].toString(), Qt::UserRole);
        if (status == "未处理") {
            if (level == "严重") {
                item->setForeground(QColor("#f44336"));
            } else if (level == "警告") {
                item->setForeground(QColor("#ff9800"));
            }
        }
    }

    m_alarmModel->appendRow(row);
}

void AlarmPlugin::onAlarmTriggered(const QString& topic, const QVariantMap& payload)
{
    Q_UNUSED(topic)
    QString alarmId = payload["alarmId"].toString();
    QString deviceName = payload["deviceName"].toString();
    QString type = payload["type"].toString();
    QString level = payload["level"].toString();

    QJsonObject alarm;
    alarm["id"] = alarmId;
    alarm["deviceName"] = deviceName;
    alarm["type"] = type;
    alarm["level"] = level;
    alarm["status"] = "未处理";
    alarm["time"] = QDateTime::currentDateTime().toString("yyyy-MM-dd HH:mm:ss");

    addAlarmItem(alarm);

    m_unreadCount++;
    emit alarmCountChanged(m_unreadCount);
    emit alarmCountChanged(m_unreadCount);
}

void AlarmPlugin::onAcknowledgeAlarm(const QString& alarmId)
{
    QVariantMap payload;
    payload["alarmId"] = alarmId;

    ServerApi::instance()->post("/api/v1/alarms/acknowledge", payload, [this](bool success, const QJsonObject&, const QString&) {
        if (success) {
            m_unreadCount = qMax(0, m_unreadCount - 1);
            emit alarmCountChanged(m_unreadCount);
            loadAlarms();
        }
    });
}

}

#include "AlarmPlugin.moc"
