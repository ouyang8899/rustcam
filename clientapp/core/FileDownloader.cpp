#include "FileDownloader.h"
#include "common/Logger.h"
#include <curl/curl.h>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QtConcurrent>
#include <QFutureWatcher>

namespace rustcam {

FileDownloader::FileDownloader(QObject* parent)
    : QObject(parent)
{
}

FileDownloader::~FileDownloader()
{
    m_cancel.store(true);
}

void FileDownloader::download(const QUrl& url, const QString& targetPath)
{
    if (m_running) {
        LOG_WARNING("FileDownloader", "download: another download is running");
        return;
    }
    if (!url.isValid() || url.isEmpty()) {
        emit finished(false, QString(), tr("下载地址无效"));
        return;
    }

    m_cancel.store(false);
    m_progress.store(0);
    m_running = true;

    QFutureWatcher<FileDownloader::DownloadResult>* watcher =
        new QFutureWatcher<FileDownloader::DownloadResult>(this);
    connect(watcher,
            &QFutureWatcher<FileDownloader::DownloadResult>::finished,
            this, [this, watcher, targetPath]() {
                const DownloadResult result = watcher->result();
                watcher->deleteLater();
                m_running = false;
                emit finished(result.success, result.targetPath, result.error);
            });
    watcher->setFuture(QtConcurrent::run([this, url, targetPath]() {
        return performDownload(url, targetPath);
    }));
}

void FileDownloader::cancel()
{
    m_cancel.store(true);
}

size_t FileDownloader::writeCallback(void* contents, size_t size, size_t nmemb, void* userp)
{
    const size_t realsize = size * nmemb;
    QFile* file = static_cast<QFile*>(userp);
    if (!file) {
        return 0;
    }
    const qint64 written = file->write(static_cast<const char*>(contents), static_cast<qint64>(realsize));
    return written < 0 ? 0 : realsize;
}

int FileDownloader::progressCallback(void* clientp,
                                     qint64 dltotal,
                                     qint64 dlnow,
                                     qint64 ultotal,
                                     qint64 ulnow)
{
    Q_UNUSED(ultotal)
    Q_UNUSED(ulnow)

    FileDownloader* self = static_cast<FileDownloader*>(clientp);
    if (!self) {
        return 0;
    }
    if (self->m_cancel.load()) {
        return 1;  // 中断传输
    }
    if (dltotal > 0) {
        const int percent = static_cast<int>(dlnow * 100 / dltotal);
        self->m_progress.store(percent);
        emit self->progressChanged(percent, static_cast<qint64>(dlnow), static_cast<qint64>(dltotal));
    }
    return 0;
}

FileDownloader::DownloadResult FileDownloader::performDownload(const QUrl& url, const QString& targetPath)
{
    DownloadResult result;
    result.targetPath = targetPath;

    const QFileInfo info(targetPath);
    QDir().mkpath(info.absolutePath());

    QFile file(targetPath);
    if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        result.error = tr("无法写入文件：%1").arg(targetPath);
        LOG_ERROR("FileDownloader", result.error);
        return result;
    }

    CURL* curl = curl_easy_init();
    if (!curl) {
        result.error = tr("curl 初始化失败");
        LOG_ERROR("FileDownloader", result.error);
        return result;
    }

    curl_easy_setopt(curl, CURLOPT_URL, url.toString().toUtf8().constData());
    curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, writeCallback);
    curl_easy_setopt(curl, CURLOPT_WRITEDATA, &file);
    curl_easy_setopt(curl, CURLOPT_NOPROGRESS, 0L);
    curl_easy_setopt(curl, CURLOPT_XFERINFOFUNCTION, progressCallback);
    // curl 的回调签名使用 curl_off_t，这里通过函数指针转换兼容
    (void)sizeof(curl_off_t);
    curl_easy_setopt(curl, CURLOPT_XFERINFODATA, this);
    curl_easy_setopt(curl, CURLOPT_FOLLOWLOCATION, 1L);
    curl_easy_setopt(curl, CURLOPT_CONNECTTIMEOUT, 15L);
    curl_easy_setopt(curl, CURLOPT_TIMEOUT, 0L);        // 大文件不设总超时
    curl_easy_setopt(curl, CURLOPT_USERAGENT, "rustcam-client/1.0");
    curl_easy_setopt(curl, CURLOPT_NOSIGNAL, 1L);
    curl_easy_setopt(curl, CURLOPT_ERRORBUFFER, nullptr);

    LOG_INFO("FileDownloader", QString("download started: %1 -> %2").arg(url.toString()).arg(targetPath));

    const CURLcode res = curl_easy_perform(curl);

    if (res != CURLE_OK) {
        const QString msg = QString("curl error: %1").arg(curl_easy_strerror(res));
        curl_easy_cleanup(curl);
        file.close();
        QFile::remove(targetPath);
        result.error = m_cancel.load() ? tr("下载已取消") : msg;
        LOG_ERROR("FileDownloader", result.error);
        return result;
    }

    long httpCode = 0;
    curl_easy_getinfo(curl, CURLINFO_RESPONSE_CODE, &httpCode);
    curl_easy_cleanup(curl);
    file.close();

    if (httpCode < 200 || httpCode >= 300) {
        QFile::remove(targetPath);
        result.error = tr("下载失败：HTTP %1").arg(httpCode);
        LOG_ERROR("FileDownloader", result.error);
        return result;
    }

    result.success = true;
    LOG_INFO("FileDownloader", QString("download finished: %1").arg(targetPath));
    return result;
}

}
