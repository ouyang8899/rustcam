#include "PlaybackSidePanel.h"
#include "ServerApi.h"
#include "common/Logger.h"
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QDateEdit>
#include <QCalendarWidget>
#include <QJsonArray>
#include <QJsonObject>
#include <QFileInfo>
#include <QtConcurrent>
#include <QFutureWatcher>
#include <QPushButton>
#include <QMessageBox>

namespace rustcam {

PlaybackSidePanel::PlaybackSidePanel(QWidget* parent)
    : QWidget(parent)
    , m_selectedDate(QDate::currentDate())
    , m_hasMore(false)
    , m_currentPage(1)
{
    setMinimumWidth(280);
    setMaximumWidth(350);
    setStyleSheet("background-color: #2b2b2b;");

    QVBoxLayout* mainLayout = new QVBoxLayout(this);
    mainLayout->setContentsMargins(4, 4, 4, 4);
    mainLayout->setSpacing(4);

    QLabel* deviceLabel = new QLabel(tr("Device List"), this);
    deviceLabel->setStyleSheet("color: #e0e0e0; font-weight: bold; padding: 4px;");
    mainLayout->addWidget(deviceLabel);

    m_tabWidget = new QTabWidget(this);
    m_tabWidget->setStyleSheet(R"(
        QTabWidget::pane {
            border: 1px solid #3e3e3e;
            background-color: #2a2a2a;
        }
        QTabBar::tab {
            background-color: #1e1e1e;
            color: #aaa;
            padding: 6px 12px;
            border: 1px solid #3e3e3e;
            border-bottom: none;
        }
        QTabBar::tab:selected {
            background-color: #2a2a2a;
            color: #fff;
        }
        QTabBar::tab:hover {
            background-color: #333;
        }
    )");

    m_regionTree = new DeviceTree(m_tabWidget);
    m_groupTree = new DeviceTree(m_tabWidget);
    m_deviceTree = new DeviceTree(m_tabWidget);

    m_tabWidget->addTab(m_regionTree, tr("Region"));
    m_tabWidget->addTab(m_groupTree, tr("Group"));
    m_tabWidget->addTab(m_deviceTree, tr("Device"));

    mainLayout->addWidget(m_tabWidget, 1);

    QLabel* timelineLabel = new QLabel(tr("Recording Timeline"), this);
    timelineLabel->setStyleSheet("color: #e0e0e0; font-weight: bold; padding: 4px;");
    mainLayout->addWidget(timelineLabel);

    QFrame* timelineFrame = new QFrame(this);
    timelineFrame->setStyleSheet("border: 1px solid #3e3e3e;");
    QVBoxLayout* timelineLayout = new QVBoxLayout(timelineFrame);
    timelineLayout->setContentsMargins(4, 4, 4, 4);
    timelineLayout->setSpacing(4);

    QHBoxLayout* dateLayout = new QHBoxLayout();
    QLabel* dateLabel = new QLabel(tr("Date:"), timelineFrame);
    dateLabel->setStyleSheet("color: #aaa;");
    QDateEdit* dateEdit = new QDateEdit(m_selectedDate, timelineFrame);
    dateEdit->setCalendarPopup(true);
    dateEdit->setStyleSheet(R"(
        QDateEdit {
            background-color: #2a2a2a;
            border: 1px solid #3e3e3e;
            border-radius: 4px;
            padding: 4px;
            color: #e0e0e0;
        }
    )");
    connect(dateEdit, &QDateEdit::dateChanged, this, [this](const QDate& date) {
        onDateChanged(date);
    });
    dateLayout->addWidget(dateLabel);
    dateLayout->addWidget(dateEdit);
    dateLayout->addStretch();
    timelineLayout->addLayout(dateLayout);

    m_timeline = new TimelineWidget(timelineFrame);
    timelineLayout->addWidget(m_timeline);
    mainLayout->addWidget(timelineFrame, 0);

    QLabel* fileListLabel = new QLabel(tr("Recording Files"), this);
    fileListLabel->setStyleSheet("color: #e0e0e0; font-weight: bold; padding: 4px;");
    mainLayout->addWidget(fileListLabel);

    QFrame* fileListFrame = new QFrame(this);
    fileListFrame->setStyleSheet("border: 1px solid #3e3e3e;");
    QVBoxLayout* fileListLayout = new QVBoxLayout(fileListFrame);
    fileListLayout->setContentsMargins(0, 0, 0, 0);
    m_fileList = new RecordingFileListWidget(fileListFrame);
    fileListLayout->addWidget(m_fileList);

    m_loadMoreBtn = new QPushButton(tr("加载更多"), fileListFrame);
    m_loadMoreBtn->setStyleSheet(R"(
        QPushButton {
            background-color: #3e3e3e;
            color: #e0e0e0;
            border: none;
            padding: 8px;
            border-radius: 4px;
        }
        QPushButton:hover {
            background-color: #4e4e4e;
        }
    )");
    m_loadMoreBtn->hide();
    fileListLayout->addWidget(m_loadMoreBtn);

    mainLayout->addWidget(fileListFrame, 1);

    connect(m_regionTree, &DeviceTree::itemDoubleClicked, this, &PlaybackSidePanel::onDeviceItemDoubleClicked);
    connect(m_groupTree, &DeviceTree::itemDoubleClicked, this, &PlaybackSidePanel::onDeviceItemDoubleClicked);
    connect(m_deviceTree, &DeviceTree::itemDoubleClicked, this, &PlaybackSidePanel::onDeviceItemDoubleClicked);
    connect(m_timeline, &TimelineWidget::timeSelected, this, &PlaybackSidePanel::onTimeSelected);
    connect(m_timeline, &TimelineWidget::periodClicked, this, &PlaybackSidePanel::onPeriodClicked);
    connect(m_fileList, &RecordingFileListWidget::fileDoubleClicked, this, &PlaybackSidePanel::onFileDoubleClicked);
    connect(m_loadMoreBtn, &QPushButton::clicked, this, &PlaybackSidePanel::onLoadMoreClicked);

    int dayStartMs = QDateTime(m_selectedDate, QTime(0, 0, 0)).toMSecsSinceEpoch();
    int dayEndMs = QDateTime(m_selectedDate, QTime(23, 59, 59)).toMSecsSinceEpoch();
    m_timeline->setTimeRange(dayStartMs, dayEndMs);
}

PlaybackSidePanel::~PlaybackSidePanel() {
}

void PlaybackSidePanel::setSelectedDate(const QDate& date) {
    m_selectedDate = date;
    int dayStartMs = QDateTime(date, QTime(0, 0, 0)).toMSecsSinceEpoch();
    int dayEndMs = QDateTime(date, QTime(23, 59, 59)).toMSecsSinceEpoch();
    m_timeline->setTimeRange(dayStartMs, dayEndMs);

    if (!m_currentDeviceId.isEmpty()) {
        loadRecordingsForDate(m_currentDeviceId, date);
    }
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

void PlaybackSidePanel::onDeviceItemDoubleClicked(QVariantMap meta) {
    QString type = meta["type"].toString();
    if (type == "channel" || type == "device") {
        QString deviceTag = meta["device_tag"].toString();
        QString channelTag = meta["channel_tag"].toString();
        QString name = meta["name"].toString();

        QString deviceId = deviceTag.isEmpty() ? channelTag : deviceTag + "/" + channelTag;
        m_currentDeviceId = deviceId;

        emit deviceSelected(deviceId, name);
        loadRecordingsForDate(deviceId, m_selectedDate);
    }
}

void PlaybackSidePanel::onDateChanged(const QDate& date) {
    m_selectedDate = date;
    emit dateChanged(date);

    int dayStartMs = QDateTime(date, QTime(0, 0, 0)).toMSecsSinceEpoch();
    int dayEndMs = QDateTime(date, QTime(23, 59, 59)).toMSecsSinceEpoch();
    m_timeline->setTimeRange(dayStartMs, dayEndMs);

    if (!m_currentDeviceId.isEmpty()) {
        loadRecordingsForDate(m_currentDeviceId, date);
    }
}

void PlaybackSidePanel::onTimeSelected(int64_t timeMs) {
    emit timeSelected(timeMs);
}

void PlaybackSidePanel::onPeriodClicked(const RecordingPeriod& period) {
    emit periodClicked(period);
}

void PlaybackSidePanel::onFileDoubleClicked(const RecordingFile& file) {
    emit fileDoubleClicked(file);
}

void PlaybackSidePanel::loadRecordingsForDate(const QString& deviceId, const QDate& date) {
    QDateTime startDt = QDateTime(date, QTime(0, 0, 0));
    QDateTime endDt = QDateTime(date, QTime(23, 59, 59));

    QString endpoint = QString("/api/v1/recordings/files?device_tag=%1&start_time=%2&end_time=%3&limit=50&page=1")
        .arg(deviceId).arg(startDt.toString(Qt::ISODate)).arg(endDt.toString(Qt::ISODate));

    QFuture<QPair<bool, QJsonObject>> future = QtConcurrent::run([endpoint]() {
        QJsonObject response;
        QString error;
        bool success = ServerApi::instance()->getSync(endpoint, response, error);
        return QPair<bool, QJsonObject>(success, response);
    });

    QFutureWatcher<QPair<bool, QJsonObject>>* watcher = new QFutureWatcher<QPair<bool, QJsonObject>>(this);
    connect(watcher, &QFutureWatcher<QPair<bool, QJsonObject>>::finished, this, [this, watcher, deviceId, date]() {
        QPair<bool, QJsonObject> result = watcher->result();
        watcher->deleteLater();

        if (!result.first) {
            LOG_ERROR("PlaybackSidePanel", QString("loadRecordingsForDate failed"));
            QMessageBox::warning(this, tr("加载失败"),
                tr("无法加载录像文件，请检查网络连接。"));
            return;
        }

        QList<RecordingPeriod> periods;
        QList<RecordingFile> files;

        QJsonObject dataObj = result.second["data"].toObject();
        QJsonArray clipsArray = dataObj["files"].toArray();
        for (const QJsonValue& v : clipsArray) {
            QJsonObject obj = v.toObject();

            QString startTimeStr = obj["start_time"].toString();
            QString endTimeStr = obj["end_time"].toString();
            QDateTime startDt = QDateTime::fromString(startTimeStr, Qt::ISODate);
            QDateTime endDt = QDateTime::fromString(endTimeStr, Qt::ISODate);
            int64_t startTimeMs = startDt.toMSecsSinceEpoch();
            int64_t endTimeMs = endDt.toMSecsSinceEpoch();

            RecordingPeriod period;
            period.startTimeMs = startTimeMs;
            period.endTimeMs = endTimeMs;
            period.url = obj["play_url"].toString();
            period.fileId = obj["id"].toString();
            periods.append(period);

            RecordingFile file;
            file.id = period.fileId;
            file.url = period.url;
            file.startTimeMs = startTimeMs;
            file.endTimeMs = endTimeMs;
            file.durationMs = obj["duration_secs"].toVariant().toLongLong() * 1000;
            QString filePath = obj["file_path"].toString();
            file.path = filePath;
            file.filename = QFileInfo(filePath).fileName();
            file.size = obj["file_size"].toVariant().toLongLong();
            file.serverTag = obj["server_tag"].toString();

            QString segType = obj["segment_type"].toString();
            if (segType == "alarm") {
                file.segmentType = SegmentType::Alarm;
            } else if (segType == "prebuffer") {
                file.segmentType = SegmentType::PreBuffer;
            } else {
                file.segmentType = SegmentType::Normal;
            }

            if (obj.contains("alarm_id") && !obj["alarm_id"].isNull()) {
                file.alarmId = obj["alarm_id"].toVariant().toLongLong();
            }

            files.append(file);
        }

        m_timeline->setPeriods(periods);
        m_fileList->setFiles(files);

        m_hasMore = dataObj["has_more"].toBool();
        m_currentPage = 1;
        m_loadMoreBtn->setVisible(m_hasMore);

        LOG_INFO("PlaybackSidePanel", QString("loadRecordingsForDate: deviceId=%1, date=%2, periods=%3, files=%4")
            .arg(deviceId).arg(date.toString("yyyy-MM-dd")).arg(periods.size()).arg(files.size()));
    });
    watcher->setFuture(future);
}

void PlaybackSidePanel::onLoadRecordingsForDevice(const QString& deviceId) {
    m_currentDeviceId = deviceId;
    loadRecordingsForDate(deviceId, m_selectedDate);
}

void PlaybackSidePanel::onLoadMoreClicked() {
    if (m_currentDeviceId.isEmpty()) return;
    int nextPage = m_currentPage + 1;
    QDateTime startDt = QDateTime(m_selectedDate, QTime(0, 0, 0));
    QDateTime endDt = QDateTime(m_selectedDate, QTime(23, 59, 59));

    QString endpoint = QString("/api/v1/recordings/files?device_tag=%1&start_time=%2&end_time=%3&limit=50&page=%4")
        .arg(m_currentDeviceId).arg(startDt.toString(Qt::ISODate)).arg(endDt.toString(Qt::ISODate)).arg(nextPage);

    QFuture<QPair<bool, QJsonObject>> future = QtConcurrent::run([endpoint]() {
        QJsonObject response;
        QString error;
        bool success = ServerApi::instance()->getSync(endpoint, response, error);
        return QPair<bool, QJsonObject>(success, response);
    });

    QFutureWatcher<QPair<bool, QJsonObject>>* watcher = new QFutureWatcher<QPair<bool, QJsonObject>>(this);
    connect(watcher, &QFutureWatcher<QPair<bool, QJsonObject>>::finished, this, [this, watcher, nextPage]() {
        QPair<bool, QJsonObject> result = watcher->result();
        watcher->deleteLater();

        if (!result.first) {
            LOG_ERROR("PlaybackSidePanel", QString("onLoadMoreClicked failed"));
            QMessageBox::warning(this, tr("加载失败"),
                tr("无法加载更多录像文件，请检查网络连接。"));
            return;
        }

        QJsonObject dataObj = result.second["data"].toObject();
        bool hasMore = dataObj["has_more"].toBool();
        m_hasMore = hasMore;
        m_currentPage = nextPage;

        QJsonArray clipsArray = dataObj["files"].toArray();
        for (const QJsonValue& v : clipsArray) {
            QJsonObject obj = v.toObject();

            QString startTimeStr = obj["start_time"].toString();
            QString endTimeStr = obj["end_time"].toString();
            QDateTime startDt = QDateTime::fromString(startTimeStr, Qt::ISODate);
            QDateTime endDt = QDateTime::fromString(endTimeStr, Qt::ISODate);
            int64_t startTimeMs = startDt.toMSecsSinceEpoch();
            int64_t endTimeMs = endDt.toMSecsSinceEpoch();

            RecordingPeriod period;
            period.startTimeMs = startTimeMs;
            period.endTimeMs = endTimeMs;
            period.url = obj["play_url"].toString();
            period.fileId = obj["id"].toString();

            RecordingFile file;
            file.id = period.fileId;
            file.url = period.url;
            file.startTimeMs = startTimeMs;
            file.endTimeMs = endTimeMs;
            file.durationMs = obj["duration_secs"].toVariant().toLongLong() * 1000;
            QString filePath = obj["file_path"].toString();
            file.path = filePath;
            file.filename = QFileInfo(filePath).fileName();
            file.size = obj["file_size"].toVariant().toLongLong();
            file.serverTag = obj["server_tag"].toString();

            QString segType = obj["segment_type"].toString();
            if (segType == "alarm") {
                file.segmentType = SegmentType::Alarm;
            } else if (segType == "prebuffer") {
                file.segmentType = SegmentType::PreBuffer;
            } else {
                file.segmentType = SegmentType::Normal;
            }

            if (obj.contains("alarm_id") && !obj["alarm_id"].isNull()) {
                file.alarmId = obj["alarm_id"].toVariant().toLongLong();
            }

            m_fileList->addFile(file);
        }

        m_loadMoreBtn->setVisible(m_hasMore);
    });
    watcher->setFuture(future);
}

}
