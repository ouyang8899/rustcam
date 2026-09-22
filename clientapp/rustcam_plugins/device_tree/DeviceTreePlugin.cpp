#include "DeviceTreePlugin.h"
#include "core/EventBus.h"
#include "core/ServerApi.h"
#include "core/SessionManager.h"
#include <QTreeView>
#include <QVBoxLayout>
#include <QLineEdit>
#include <QPushButton>
#include <QLabel>
#include <QStandardItem>
#include <QHeaderView>
#include <QMenu>
#include <QContextMenuEvent>

namespace rustcam {

class DeviceTreeWidget : public QWidget {
    Q_OBJECT
public:
    explicit DeviceTreeWidget(DeviceTreePlugin* plugin, QWidget* parent = nullptr)
        : QWidget(parent)
        , m_plugin(plugin)
    {
        setupUi();
    }

private slots:
    void onSearchChanged(const QString& text) {
        m_plugin->onSearchTextChanged(text);
    }

    void onRefreshClicked() {
        m_plugin->onRefreshDevices();
    }

    void onCustomContextMenu(const QPoint& pos) {
        QMenu menu(this);
        menu.addAction("打开视频", this, [this]() {
            QModelIndex index = m_treeView->indexAt(pos);
            if (index.isValid()) {
                QString deviceId = index.data(Qt::UserRole).toString();
                if (!deviceId.isEmpty()) {
                    m_plugin->onDeviceSelected(deviceId);
                }
            }
        });
        menu.addAction("查看录像", this, [this]() {
            QModelIndex index = m_treeView->indexAt(pos);
            if (index.isValid()) {
                QString deviceId = index.data(Qt::UserRole).toString();
                if (!deviceId.isEmpty()) {
                    QVariantMap payload;
                    payload["deviceId"] = deviceId;
                    EventBus::instance()->publish("playback/request", payload);
                }
            }
        });
        menu.exec(m_treeView->mapToGlobal(pos));
    }

private:
    void setupUi() {
        QVBoxLayout* layout = new QVBoxLayout(this);
        layout->setContentsMargins(8, 8, 8, 8);
        layout->setSpacing(8);

        QHBoxLayout* searchLayout = new QHBoxLayout();
        QLabel* titleLabel = new QLabel("设备列表", this);
        titleLabel->setStyleSheet(R"(
            QLabel {
                color: #e0e0e0;
                font-size: 14px;
                font-weight: bold;
                font-family: 'Microsoft YaHei', sans-serif;
            }
        )");
        searchLayout->addWidget(titleLabel);
        searchLayout->addStretch();

        QPushButton* refreshBtn = new QPushButton("刷新", this);
        refreshBtn->setCursor(Qt::PointingHandCursor);
        refreshBtn->setStyleSheet(R"(
            QPushButton {
                background-color: transparent;
                color: #0078D4;
                border: none;
                font-size: 12px;
            }
            QPushButton:hover {
                color: #1084d8;
            }
        )");
        connect(refreshBtn, &QPushButton::clicked, this, &DeviceTreeWidget::onRefreshClicked);
        searchLayout->addWidget(refreshBtn);
        layout->addLayout(searchLayout);

        m_searchEdit = new QLineEdit(this);
        m_searchEdit->setPlaceholderText("搜索设备...");
        m_searchEdit->setStyleSheet(R"(
            QLineEdit {
                background-color: #2b2b2b;
                border: 1px solid #3e3e3e;
                border-radius: 4px;
                padding: 6px 12px;
                color: #e0e0e0;
                font-size: 13px;
            }
            QLineEdit:focus {
                border-color: #0078D4;
            }
        )");
        connect(m_searchEdit, &QLineEdit::textChanged, this, &DeviceTreeWidget::onSearchChanged);
        layout->addWidget(m_searchEdit);

        m_treeView = new QTreeView(this);
        m_treeView->setModel(m_plugin->deviceModel());
        m_treeView->setHeaderHidden(false);
        m_treeView->setStyleSheet(R"(
            QTreeView {
                background-color: #1a1a1a;
                border: 1px solid #3e3e3e;
                border-radius: 4px;
                color: #e0e0e0;
                font-size: 13px;
            }
            QTreeView::item {
                padding: 4px 0;
            }
            QTreeView::item:selected {
                background-color: rgba(0, 120, 212, 0.3);
            }
            QTreeView::item:hover {
                background-color: rgba(255, 255, 255, 0.04);
            }
        )");
        m_treeView->setContextMenuPolicy(Qt::CustomContextMenu);
        connect(m_treeView, &QTreeView::customContextMenuRequested, this, &DeviceTreeWidget::onCustomContextMenu);
        connect(m_treeView, &QTreeView::doubleClicked, this, [this](const QModelIndex& index) {
            QString deviceId = index.data(Qt::UserRole).toString();
            if (!deviceId.isEmpty()) {
                m_plugin->onDeviceSelected(deviceId);
            }
        });
        layout->addWidget(m_treeView);
    }

    DeviceTreePlugin* m_plugin;
    QLineEdit* m_searchEdit;
    QTreeView* m_treeView;
};

bool DeviceTreePlugin::initialize(EventBus* bus, QObject* cfg)
{
    Q_UNUSED(cfg)
    m_deviceModel = new QStandardItemModel(this);
    m_deviceModel->setHorizontalHeaderLabels({"设备名称", "状态", "类型"});

    loadDevices();

    return true;
}

void DeviceTreePlugin::shutdown()
{
    EventBus::instance()->unsubscribeAll(this);
}

QWidget* DeviceTreePlugin::createSidePanel(QWidget* parent)
{
    return new DeviceTreeWidget(this, parent);
}

void DeviceTreePlugin::loadDevices()
{
    ServerApi::instance()->get("/api/v1/devices", [this](bool success, const QJsonObject& resp, const QString&) {
        if (success) {
            QJsonArray devices = resp["data"].toArray();
            buildDeviceTree(devices, nullptr);
            LOG_INFO("DeviceTreePlugin", QString("Loaded %1 devices").arg(devices.size()));
        }
    });
}

void DeviceTreePlugin::buildDeviceTree(const QJsonArray& devices, QStandardItem* parent)
{
    if (!parent) {
        m_deviceModel->clear();
        m_deviceModel->setHorizontalHeaderLabels({"设备名称", "状态", "类型"});
    }

    for (const auto& dev : devices) {
        QJsonObject obj = dev.toObject();
        QString id = obj["id"].toString();
        QString name = obj["name"].toString();
        QString type = obj["type"].toString();
        bool online = obj["online"].toBool();

        QStandardItem* item = new QStandardItem();
        item->setText(name);
        item->setData(id, Qt::UserRole);

        QStandardItem* statusItem = new QStandardItem();
        statusItem->setText(online ? "在线" : "离线");
        statusItem->setForeground(online ? QColor("#4caf50") : QColor("#f44336"));

        QStandardItem* typeItem = new QStandardItem();
        typeItem->setText(type);

        if (parent) {
            parent->appendRow({item, statusItem, typeItem});
        } else {
            m_deviceModel->appendRow({item, statusItem, typeItem});
        }

        if (obj.contains("children")) {
            buildDeviceTree(obj["children"].toArray(), item);
        }
    }
}

QStandardItem* DeviceTreePlugin::findDeviceItem(QStandardItem* parent, const QString& deviceId)
{
    for (int i = 0; i < parent->rowCount(); ++i) {
        QStandardItem* child = parent->child(i);
        if (child->data(Qt::UserRole).toString() == deviceId) {
            return child;
        }
        if (child->hasChildren()) {
            QStandardItem* found = findDeviceItem(child, deviceId);
            if (found) return found;
        }
    }
    return nullptr;
}

void DeviceTreePlugin::onDeviceSelected(const QString& deviceId)
{
    QVariantMap payload;
    payload["deviceId"] = deviceId;
    EventBus::instance()->publish("video/stream/request", payload);
}

void DeviceTreePlugin::onRefreshDevices()
{
    loadDevices();
}

void DeviceTreePlugin::onSearchTextChanged(const QString& text)
{
    m_searchText = text;
    QStandardItem* root = m_deviceModel->invisibleRootItem();
    for (int i = 0; i < root->rowCount(); ++i) {
        QStandardItem* item = root->child(i);
        QString name = item->text();
        bool match = name.contains(text, Qt::CaseInsensitive);
        item->setAccessibleText(match ? "match" : "nomatch");
        item->setData(match || text.isEmpty() ? -1 : 0, Qt::UserRole + 1);
    }
}

}

#include "DeviceTreePlugin.moc"
