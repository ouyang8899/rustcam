#pragma once

#include <QObject>
#include <QJsonObject>
#include <QList>
#include <QString>
#include <functional>
#include "CoreExport.h"
#include "RecordingTypes.h"

namespace rustcam {

/// 录像分段的一页查询结果
/// 一条报警（时间轴上的事件标记）
struct AlarmEvent {
    qint64 id = 0;
    int64_t timeMs = 0;
    QString alarmType;
    QString description;
};

struct RecordingFilePage {
    QList<RecordingFile> files;
    int total = 0;
    int page = 1;
    int limit = 50;
    bool hasMore = false;
};

/// 录像查询的唯一入口。
///
/// 回放相关的所有服务端请求都走这里，保证：
///  - device_tag / channel_tag 的口径与服务端一致（精确匹配，不拼接）；
///  - play_url 由服务端给出完整地址，客户端不自己拼；
///  - 分页语义（page / limit / total / has_more）只有一处实现。
class RUSTCAM_CORE_API RecordingApi : public QObject {
    Q_OBJECT
public:
    using FilesCallback = std::function<void(bool success, const RecordingFilePage& page, const QString& error)>;
    using UrlCallback = std::function<void(bool success, const QString& url, const QString& error)>;

    static RecordingApi* instance();

    /// 查询录像分段
    /// @param deviceTag   设备标识，可空
    /// @param channelTag  通道标识，可空
    /// @param startMs     起始时间，<=0 表示不限
    /// @param endMs       结束时间，<=0 表示不限
    /// @param segmentType 分段类型（normal/alarm/prebuffer），空表示全部
    /// @param page        页码，从 1 开始
    void queryFiles(const QString& deviceTag,
                   const QString& channelTag,
                   qint64 startMs,
                   qint64 endMs,
                   const QString& segmentType,
                   int page,
                   int limit,
                   FilesCallback callback);

    /// 取单个录像的播放地址（服务端返回完整地址）
    void resolvePlayUrl(const QString& fileId, UrlCallback callback);

    /// 查询时间范围内的报警（用于在时间轴上叠加事件标记）
    void queryAlarms(const QString& deviceTag,
                     qint64 startMs,
                     qint64 endMs,
                     std::function<void(bool, const QList<AlarmEvent>&, const QString&)> callback);

    /// 解析服务端返回的分段
    static RecordingFile parseFile(const QJsonObject& obj);
    /// 解析一页的分段
    static QList<RecordingFile> parseFiles(const QJsonArray& array);

    /// 把分页结果解析成结构体
    static RecordingFilePage parsePage(const QJsonObject& dataObj);

private:
    explicit RecordingApi(QObject* parent = nullptr);
    ~RecordingApi();
    Q_DISABLE_COPY(RecordingApi)
};

}
