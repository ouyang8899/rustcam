#pragma once

#include <QWidget>
#include <QList>
#include <QVariantMap>
#include <QDate>
#include <QTabWidget>
#include "CoreExport.h"
#include "DeviceTree.h"
#include "TimelineWidget.h"
#include "RecordingFileListWidget.h"

namespace rustcam {

class RUSTCAM_CORE_API PlaybackSidePanel : public QWidget {
    Q_OBJECT
public:
    explicit PlaybackSidePanel(QWidget* parent = nullptr);
    ~PlaybackSidePanel();

    DeviceTree* regionTree() const { return m_regionTree; }
    DeviceTree* groupTree() const { return m_groupTree; }
    DeviceTree* deviceTree() const { return m_deviceTree; }
    TimelineWidget* timeline() const { return m_timeline; }
    RecordingFileListWidget* fileList() const { return m_fileList; }

    void setSelectedDate(const QDate& date);
    void setRegionTreeNodes(const QList<DeviceTreeNode::Ptr>& nodes);
    void setGroupTreeNodes(const QList<DeviceTreeNode::Ptr>& nodes);
    void setDeviceTreeNodes(const QList<DeviceTreeNode::Ptr>& nodes);

signals:
    void deviceSelected(const QString& deviceId, const QString& deviceName);
    void dateChanged(const QDate& date);
    void timeSelected(int64_t timeMs);
    void periodClicked(const RecordingPeriod& period);
    void filePlayRequested(const RecordingFile& file);
    void fileDoubleClicked(const RecordingFile& file);

private slots:
    void onDeviceItemDoubleClicked(QVariantMap meta);
    void onDateChanged(const QDate& date);
    void onTimeSelected(int64_t timeMs);
    void onPeriodClicked(const RecordingPeriod& period);
    void onFileDoubleClicked(const RecordingFile& file);
    void onLoadRecordingsForDevice(const QString& deviceId);

private:
    void loadRecordingsForDate(const QString& deviceId, const QDate& date);

    QTabWidget* m_tabWidget;
    DeviceTree* m_regionTree;
    DeviceTree* m_groupTree;
    DeviceTree* m_deviceTree;
    TimelineWidget* m_timeline;
    RecordingFileListWidget* m_fileList;
    QDate m_selectedDate;
    QString m_currentDeviceId;
};

}
