#pragma once

#include "interfaces/IPlugin.h"
#include <QObject>
#include <QString>
#include <QJsonArray>
#include <QStandardItemModel>

namespace rustcam {

class EventBus;

class DeviceTreePlugin : public QObject, public IPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID "com.rustcam.IPlugin/1.0" FILE "plugin.json")
    Q_INTERFACES(rustcam::IPlugin)
public:
    QString id() const override { return "device_tree"; }
    QString displayName() const override { return tr("设备树"); }
    QString version() const override { return "1.0.0"; }
    QIcon icon() const override { return QIcon(":/icons/device_tree.svg"); }

    bool initialize(EventBus* bus, QObject* cfg) override;
    void shutdown() override;
    QWidget* createWorkspace(QWidget* parent) override { Q_UNUSED(parent); return nullptr; }
    QWidget* createSidePanel(QWidget* parent) override;
    bool isWorkspace() const override { return true; }

    QStandardItemModel* deviceModel() const { return m_deviceModel; }

private slots:
    void onDeviceSelected(const QString& deviceId);
    void onRefreshDevices();
    void onSearchTextChanged(const QString& text);

private:
    void loadDevices();
    void buildDeviceTree(const QJsonArray& devices, QStandardItem* parent);
    QStandardItem* findDeviceItem(QStandardItem* parent, const QString& deviceId);

    QStandardItemModel* m_deviceModel;
    QString m_searchText;
};

}
