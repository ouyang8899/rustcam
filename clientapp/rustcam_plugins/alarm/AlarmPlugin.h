#pragma once

#include "interfaces/IPlugin.h"
#include <QObject>
#include <QString>
#include <QStandardItemModel>
#include <QJsonArray>

namespace rustcam {

class AlarmPlugin : public QObject, public IPlugin {
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
    void onAlarmTriggered(const QString& topic, const QVariantMap& payload);
    void onRefreshAlarms();
    void onAcknowledgeAlarm(const QString& alarmId);

private:
    void loadAlarms();
    void addAlarmItem(const QJsonObject& alarm);

    QStandardItemModel* m_alarmModel;
    int m_unreadCount;
};

}
