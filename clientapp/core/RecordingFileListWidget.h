#pragma once

#include <QWidget>
#include <QList>
#include <QAbstractListModel>
#include <QListView>
#include <QLabel>
#include <QVBoxLayout>
#include <QHBoxLayout>
#include "CoreExport.h"

namespace rustcam {

struct RecordingFile {
    QString id;
    QString filename;
    QString path;
    QString url;
    int64_t size = 0;
    int64_t durationMs = 0;
    int64_t startTimeMs = 0;
    int64_t endTimeMs = 0;
    QString mediaServerName;
};

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
        MediaServerRole
    };

    explicit RecordingFileListModel(QObject* parent = nullptr);

    int rowCount(const QModelIndex& parent = QModelIndex()) const override;
    int columnCount(const QModelIndex& parent = QModelIndex()) const override;
    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
    QVariant headerData(int section, Qt::Orientation orientation, int role = Qt::DisplayRole) const override;

    void setFiles(const QList<RecordingFile>& files);
    void clearFiles();
    RecordingFile fileAt(int index) const;

signals:
    void fileSelected(const RecordingFile& file);

private:
    QList<RecordingFile> m_files;
};

class RUSTCAM_CORE_API RecordingFileListWidget : public QWidget {
    Q_OBJECT
public:
    explicit RecordingFileListWidget(QWidget* parent = nullptr);
    ~RecordingFileListWidget();

    void setFiles(const QList<RecordingFile>& files);
    void clearFiles();

signals:
    void fileDoubleClicked(const RecordingFile& file);
    void fileClicked(const RecordingFile& file);

private slots:
    void onItemDoubleClicked(const QModelIndex& index);
    void onItemClicked(const QModelIndex& index);

private:
    QListView* m_listView;
    RecordingFileListModel* m_model;
    QLabel* m_countLabel;
};

}
