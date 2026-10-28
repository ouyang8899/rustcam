#pragma once

#include "interfaces/IPlugin.h"
#include <QObject>
#include <QString>
#include <QJsonArray>
#include "core/EventBus.h"
#include "core/AlarmSidePanel.h"
#include "core/DataManager.h"

class QLabel;

namespace rustcam {

class AlarmPlugin : public IPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID "com.rustcam.IPlugin/1.0" FILE "plugin.json")
    Q_INTERFACES(rustcam::IPlugin)
public:
    QString id() const override { return "alarm"; }
    QString displayName() const override { return tr("告警中心"); }
    QString version() const override { return "1.0.0"; }

    bool initialize(EventBus* bus, QObject* cfg) override;
    void shutdown() override;
    QWidget* createWorkspace(QWidget* parent) override;
    QWidget* createSidePanel(QWidget* parent) override;
    bool isWorkspace() const override { return true; }

    int unreadCount() const { return m_unreadCount; }

signals:
    void alarmCountChanged(int count);

private slots:
    void onAlarmTriggered(const QString& topic, EventDataPtr data);
    void onDataManagerLoaded();
    void onDeviceSelected(const QString& deviceId, const QString& deviceName);
    void onAlarmSelected(const QString& alarmId);
    void onRefreshClicked();
    void onFilterChanged(const QString& text);
    void onAlarmCountChanged(int count);

private:
    void loadAlarms();
    void addAlarmItem(const QJsonObject& alarm);

    QWidget* m_workspace;
    AlarmSidePanel* m_sidePanel;
    /// 工具栏上的未处理计数标签
    QLabel* m_countLabel = nullptr;
    int m_unreadCount = 0;
    QString m_currentDeviceId;
};

}
