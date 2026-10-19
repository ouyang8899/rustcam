#pragma once

#include <QWidget>
#include <QList>
#include <QVariantMap>
#include <QTabWidget>
#include <QStandardItemModel>
#include <QJsonObject>
#include <QJsonArray>
#include <QLabel>
#include <QPushButton>
#include <QComboBox>
#include "CoreExport.h"
#include "DeviceTree.h"

namespace rustcam {

class RUSTCAM_CORE_API AlarmSidePanel : public QWidget {
    Q_OBJECT
public:
    explicit AlarmSidePanel(QWidget* parent = nullptr);
    ~AlarmSidePanel();

    DeviceTree* regionTree() const { return m_regionTree; }
    DeviceTree* groupTree() const { return m_groupTree; }
    DeviceTree* deviceTree() const { return m_deviceTree; }

    void setRegionTreeNodes(const QList<DeviceTreeNode::Ptr>& nodes);
    void setGroupTreeNodes(const QList<DeviceTreeNode::Ptr>& nodes);
    void setDeviceTreeNodes(const QList<DeviceTreeNode::Ptr>& nodes);

    void loadAlarms(int page = 1, int pageSize = 20);
    void setFilter(const QString& filter) { m_filter = filter; }

    QStandardItemModel* alarmModel() const { return m_alarmModel; }

signals:
    void deviceSelected(const QString& deviceId, const QString& deviceName);
    void alarmSelected(const QString& alarmId);
    void pageChanged(int page);
    void filterChanged(const QString& filter);

private slots:
    void onDeviceItemDoubleClicked(QVariantMap meta);
    void onAlarmClicked(const QModelIndex& index);
    void onPrevPage();
    void onNextPage();
    void onPageSizeChanged(int size);
    void onAlarmReply(bool success, const QJsonObject& resp, const QString& error);

private:
    void setupAlarmList();

    QTabWidget* m_tabWidget;
    DeviceTree* m_regionTree;
    DeviceTree* m_groupTree;
    DeviceTree* m_deviceTree;

    QStandardItemModel* m_alarmModel;
    QTableView* m_alarmTable;
    QLabel* m_pageLabel;
    QPushButton* m_prevBtn;
    QPushButton* m_nextBtn;
    QComboBox* m_pageSizeCombo;

    int m_currentPage = 1;
    int m_totalPages = 1;
    int m_totalCount = 0;
    int m_pageSize = 20;
    QString m_filter;
    QString m_currentDeviceId;
};

}
