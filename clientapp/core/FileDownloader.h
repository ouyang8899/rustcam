#pragma once

#include <QObject>
#include <QString>
#include <QUrl>
#include <atomic>
#include "CoreExport.h"

class QFile;

namespace rustcam {

/// 下载远程文件（录像文件本体）到本地，基于 libcurl。
///
/// 播放走 http 直连，不需要下载；这个类用于「下载」以及「剪辑导出」的前置步骤。
class RUSTCAM_CORE_API FileDownloader : public QObject {
    Q_OBJECT
public:
    explicit FileDownloader(QObject* parent = nullptr);
    ~FileDownloader();

    /// 开始下载，保存到 targetPath（在工作线程执行，不阻塞界面）
    void download(const QUrl& url, const QString& targetPath);

    /// 取消当前下载
    void cancel();

    int progress() const { return m_progress; }
    bool isRunning() const { return m_running; }

signals:
    void progressChanged(int percent, qint64 received, qint64 total);
    void finished(bool success, const QString& targetPath, const QString& error);

private:
    struct DownloadResult {
        bool success = false;
        QString error;
        QString targetPath;
    };

    static size_t writeCallback(void* contents, size_t size, size_t nmemb, void* userp);
    static int progressCallback(void* clientp,
                               qint64 dltotal,
                               qint64 dlnow,
                               qint64 ultotal,
                               qint64 ulnow);
    DownloadResult performDownload(const QUrl& url, const QString& targetPath);

    std::atomic_bool m_cancel { false };
    std::atomic_int m_progress { 0 };
    bool m_running = false;
};

}
