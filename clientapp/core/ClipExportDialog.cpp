#include "ClipExportDialog.h"
#include "ClipExporter.h"
#include "FileDownloader.h"
#include "common/Logger.h"
#include <QDialogButtonBox>
#include <QDir>
#include <QFileDialog>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QProgressBar>
#include <QPushButton>
#include <QSpinBox>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QVBoxLayout>
#include <QtConcurrent>
#include <QFutureWatcher>

namespace rustcam {

ClipExportDialog::ClipExportDialog(const RecordingFile& file, QWidget* parent)
    : QDialog(parent)
    , m_file(file)
{
    setWindowTitle(tr("导出录像片段"));
    setMinimumWidth(420);

    QVBoxLayout* layout = new QVBoxLayout(this);

    QLabel* info = new QLabel(
        tr("源文件：%1\n时长：%2 秒")
            .arg(file.filename.isEmpty() ? tr("未知") : file.filename)
            .arg(file.durationMs / 1000),
        this);
    info->setStyleSheet("color: #bbb;");
    layout->addWidget(info);

    QHBoxLayout* rangeLayout = new QHBoxLayout();
    rangeLayout->addWidget(new QLabel(tr("起点(秒)"), this));
    m_startSpin = new QSpinBox(this);
    m_startSpin->setRange(0, 24 * 3600);
    m_startSpin->setValue(0);
    rangeLayout->addWidget(m_startSpin);
    rangeLayout->addWidget(new QLabel(tr("终点(秒)"), this));
    m_endSpin = new QSpinBox(this);
    m_endSpin->setRange(1, 24 * 3600);
    m_endSpin->setValue(static_cast<int>(file.durationMs / 1000));
    rangeLayout->addWidget(m_endSpin);
    rangeLayout->addStretch();
    layout->addLayout(rangeLayout);

    QHBoxLayout* outputLayout = new QHBoxLayout();
    outputLayout->addWidget(new QLabel(tr("输出文件"), this));
    m_outputEdit = new QLineEdit(this);
    const QString suggested = QDir(QStandardPaths::writableLocation(QStandardPaths::DownloadLocation))
                                 .filePath(file.filename.isEmpty()
                                                ? QString("clip.mp4")
                                                : QString("clip_") + file.filename);
    m_outputEdit->setText(suggested);
    outputLayout->addWidget(m_outputEdit, 1);
    m_browseBtn = new QPushButton(tr("浏览"), this);
    outputLayout->addWidget(m_browseBtn);
    layout->addLayout(outputLayout);

    m_progress = new QProgressBar(this);
    m_progress->setRange(0, 100);
    m_progress->setVisible(false);
    layout->addWidget(m_progress);

    m_status = new QLabel(QString(), this);
    m_status->setStyleSheet("color: #aaa;");
    layout->addWidget(m_status);

    QHBoxLayout* buttonLayout = new QHBoxLayout();
    buttonLayout->addStretch();
    QDialogButtonBox* buttons = new QDialogButtonBox(QDialogButtonBox::Close, this);
    buttonLayout->addWidget(buttons);
    m_exportBtn = new QPushButton(tr("导出"), this);
    m_exportBtn->setProperty("class", "PrimaryButton");
    buttonLayout->addWidget(m_exportBtn);
    layout->addLayout(buttonLayout);

    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(m_browseBtn, &QPushButton::clicked, this, &ClipExportDialog::onBrowseOutput);
    connect(m_exportBtn, &QPushButton::clicked, this, &ClipExportDialog::onExport);

    m_downloader = new FileDownloader(this);
    connect(m_downloader, &FileDownloader::progressChanged, this, [this](int percent, qint64, qint64) {
        m_progress->setValue(percent);
    });
    connect(m_downloader, &FileDownloader::finished, this, &ClipExportDialog::onDownloadFinished);
}

ClipExportDialog::~ClipExportDialog()
{
}

void ClipExportDialog::setBusy(bool busy, const QString& status)
{
    m_exportBtn->setEnabled(!busy);
    m_browseBtn->setEnabled(!busy);
    m_startSpin->setEnabled(!busy);
    m_endSpin->setEnabled(!busy);
    m_progress->setVisible(busy);
    m_status->setText(status);
}

void ClipExportDialog::onBrowseOutput()
{
    const QString path = QFileDialog::getSaveFileName(this, tr("保存导出片段"), m_outputEdit->text());
    if (!path.isEmpty()) {
        m_outputEdit->setText(path);
    }
}

void ClipExportDialog::onExport()
{
    if (m_file.url.isEmpty()) {
        QMessageBox::warning(this, tr("无法导出"), tr("该录像文件没有可用的播放地址。"));
        return;
    }
    const int64_t startMs = static_cast<int64_t>(m_startSpin->value()) * 1000;
    const int64_t endMs = static_cast<int64_t>(m_endSpin->value()) * 1000;
    if (endMs <= startMs) {
        QMessageBox::warning(this, tr("时间范围无效"), tr("终点必须晚于起点。"));
        return;
    }
    const QString output = m_outputEdit->text().trimmed();
    if (output.isEmpty()) {
        QMessageBox::warning(this, tr("无法导出"), tr("请选择输出文件。"));
        return;
    }

    m_startMs = startMs;
    m_endMs = endMs;
    m_outputPath = output;

    setBusy(true, tr("正在下载源文件..."));

    // 先把源文件下载到本地，再交给 FFmpeg 裁剪。
    // 临时目录是成员变量，保证下载/裁剪期间不会被删除。
    if (!m_tempDir.isValid()) {
        setBusy(false, tr("导出失败"));
        QMessageBox::warning(this, tr("导出失败"), tr("无法创建临时目录。"));
        return;
    }
    const QString source =
        m_tempDir.filePath(m_file.filename.isEmpty() ? QString("source.mp4") : m_file.filename);

    m_downloader->download(QUrl(m_file.url), source);
}

void ClipExportDialog::onDownloadFinished(bool ok, const QString& path, const QString& error)
{
    if (!ok) {
        setBusy(false, tr("下载失败"));
        QMessageBox::warning(this, tr("导出失败"),
            tr("下载源文件失败：%1").arg(error));
        return;
    }
    setBusy(false, tr("正在裁剪..."));
    startTrim(path);
}

void ClipExportDialog::startTrim(const QString& sourcePath)
{
    const QString source = sourcePath;
    const QString output = m_outputPath;
    const int64_t startMs = m_startMs;
    const int64_t endMs = m_endMs;

    // 裁剪比较耗时，放到后台线程，避免界面卡死
    using TrimResult = QPair<QPair<bool, QString>, qint64>;
    QFutureWatcher<TrimResult>* watcher = new QFutureWatcher<TrimResult>(this);
    connect(watcher, &QFutureWatcher<TrimResult>::finished, this,
            [this, watcher, startMs]() {
                const TrimResult result = watcher->result();
                watcher->deleteLater();
                setBusy(false, result.first.first ? tr("导出完成") : tr("导出失败"));
                if (!result.first.first) {
                    QMessageBox::warning(this, tr("导出失败"), result.first.second);
                    return;
                }
                const qint64 actualStart = result.second;
                if (actualStart > startMs + 200) {
                    QMessageBox::information(this, tr("导出完成"),
                        tr("片段已保存到：%1\n\n注意：起点已对齐到最近的关键帧，"
                           "实际起点为 %2 秒（请求 %3 秒），开头少量内容被裁掉。")
                            .arg(m_outputPath)
                            .arg(actualStart / 1000.0, 0, 'f', 1)
                            .arg(startMs / 1000.0, 0, 'f', 1));
                } else {
                    QMessageBox::information(this, tr("导出完成"),
                        tr("片段已保存到：%1").arg(m_outputPath));
                }
                accept();
            });
    watcher->setFuture(QtConcurrent::run([this, source, output, startMs, endMs]() {
        QString clipError;
        int64_t actualStartMs = startMs;
        const bool ok = ClipExporter::instance()->exportClip(
            source, output, startMs, endMs, clipError,
            [this](int percent, const QString& stage) {
                QMetaObject::invokeMethod(this, [this, percent, stage]() {
                    m_progress->setValue(percent);
                    m_status->setText(stage);
                }, Qt::QueuedConnection);
            },
            &actualStartMs);
        return TrimResult(QPair<bool, QString>(ok, clipError), actualStartMs);
    }));
}

}
