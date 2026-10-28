#pragma once

#include <QWidget>
#include <QList>
#include <QAbstractListModel>
#include <QComboBox>
#include <QListView>
#include <QLabel>
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QStyledItemDelegate>
#include <QMimeData>
#include "CoreExport.h"
#include "TimelineWidget.h"
#include "RecordingTypes.h"

namespace rustcam {

class RecordingFileListModel : public QAbstractListModel {
    Q_OBJECT
public:
    enum Roles {
        IdRole = Qt::UserRole + 1,
        FilenameRole,
        PathRole,
        UrlRole,
        SizeRole,
        DurationRole,
        StartTimeRole,
        EndTimeRole,
        MediaServerRole,
        SegmentTypeRole,
        AlarmIdRole
    };

    explicit RecordingFileListModel(QObject* parent = nullptr);

    int rowCount(const QModelIndex& parent = QModelIndex()) const override;
    int columnCount(const QModelIndex& parent = QModelIndex()) const override;
    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
    QVariant headerData(int section, Qt::Orientation orientation, int role = Qt::DisplayRole) const override;
    /// 支持拖到播放窗口：拖动时带上通道与文件信息
    Qt::ItemFlags flags(const QModelIndex& index) const override;
    QMimeData* mimeData(const QModelIndexList& indexes) const override;

    /// 所属通道（"deviceTag/channelTag"）
    void setChannelKey(const QString& key) { m_channelKey = key; }
    QString channelKey() const { return m_channelKey; }

    void setFiles(const QList<RecordingFile>& files);
    void addFile(const RecordingFile& file);
    void clearFiles();
    RecordingFile fileAt(int index) const;

signals:
    void fileSelected(const RecordingFile& file);

private:
    QList<RecordingFile> m_files;
    QString m_channelKey;
};

/// 录像文件条目：两行显示
///   11:34:15 - 11:39:17   30:10
///   2026-10-23-11-34-15-1.mp4   39.7 MB  [报警]
class RecordingFileDelegate : public QStyledItemDelegate {
    Q_OBJECT
public:
    explicit RecordingFileDelegate(QObject* parent = nullptr);

    void paint(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const override;
    QSize sizeHint(const QStyleOptionViewItem& option, const QModelIndex& index) const override;
};

class RUSTCAM_CORE_API RecordingFileListWidget : public QWidget {
    Q_OBJECT
public:
    explicit RecordingFileListWidget(QWidget* parent = nullptr);
    ~RecordingFileListWidget();

    void setFiles(const QList<RecordingFile>& files);
    void addFile(const RecordingFile& file);
    void clearFiles();

    /// 所属通道（"deviceTag/channelTag"），拖到播放窗口时带上
    void setChannelKey(const QString& key);
    QString channelKey() const;
    /// 当前选中行对应的录像（导出片段用）
    RecordingFile currentFile() const;

    /// 更新分页信息：total 为服务端返回的总数，loaded 为已加载条数
    void setPagingInfo(int total, int loaded);
    /// 每页条数
    int pageSize() const { return m_pageSize; }
    void setPageSize(int size);

signals:
    void fileDoubleClicked(const RecordingFile& file);
    void fileClicked(const RecordingFile& file);
    /// 用户请求下载某个录像文件
    void downloadRequested(const RecordingFile& file);
    /// 每页条数变化，需要重新查询
    void pageSizeChanged(int size);

private slots:
    void onItemDoubleClicked(const QModelIndex& index);
    void onItemClicked(const QModelIndex& index);
    void onContextMenu(const QPoint& pos);

private:
    QListView* m_listView;
    RecordingFileListModel* m_model;
    QLabel* m_countLabel;
    QComboBox* m_pageSizeCombo;
    /// 每次向服务端请求的条数（服务端上限 500）
    int m_pageSize = 500;
    int m_total = 0;
};

}
