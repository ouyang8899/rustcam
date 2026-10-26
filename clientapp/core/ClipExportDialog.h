#pragma once

#include <QDialog>
#include <QString>
#include <QTemporaryDir>
#include "CoreExport.h"
#include "RecordingTypes.h"

class QLabel;
class QSpinBox;
class QLineEdit;
class QPushButton;
class QProgressBar;

namespace rustcam {

class FileDownloader;

/// 录像片段导出对话框：选择时间区间 -> 下载源文件 -> FFmpeg 裁剪 -> 保存
class RUSTCAM_CORE_API ClipExportDialog : public QDialog {
    Q_OBJECT
public:
    explicit ClipExportDialog(const RecordingFile& file, QWidget* parent = nullptr);
    ~ClipExportDialog();

private slots:
    void onBrowseOutput();
    void onExport();
    void onDownloadFinished(bool success, const QString& path, const QString& error);

private:
    void setBusy(bool busy, const QString& status);
    void startTrim(const QString& sourcePath);

    RecordingFile m_file;
    FileDownloader* m_downloader = nullptr;
    QSpinBox* m_startSpin = nullptr;
    QSpinBox* m_endSpin = nullptr;
    QLineEdit* m_outputEdit = nullptr;
    QPushButton* m_exportBtn = nullptr;
    QPushButton* m_browseBtn = nullptr;
    QProgressBar* m_progress = nullptr;
    QLabel* m_status = nullptr;
    /// 下载源文件用的临时目录，必须比下载过程活得久
    QTemporaryDir m_tempDir;
    /// 裁剪参数（在异步裁剪时传递）
    int64_t m_startMs = 0;
    int64_t m_endMs = 0;
    QString m_outputPath;
};

}
