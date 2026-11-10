#pragma once

#include "TourScheme.h"

#include <QJsonObject>
#include <QList>
#include <QObject>
#include <QString>
#include <QVector>
#include <functional>

#include "PlayerLayout.h"

namespace rustcam {

/// 视频轮询方案的 REST 封装。
///
/// ServerApi 的请求是阻塞式的，因此所有请求都在工作线程发起，
/// 回调统一回到 TourApi 所在线程（插件线程）执行。
class TourApi : public QObject {
    Q_OBJECT
public:
    using SchemeListCb = std::function<void(bool, QVector<TourScheme>, QString)>;
    using SchemeCb = std::function<void(bool, TourScheme, QString)>;
    using VoidCb = std::function<void(bool, QString)>;
    using UrlCb = std::function<void(bool, QString, QString)>;
    using LayoutCb = std::function<void(bool, QList<PlayerLayout>, QString)>;

    explicit TourApi(QObject* parent = nullptr);
    ~TourApi() override;

    void listSchemes(SchemeListCb cb);
    void getScheme(qint64 id, SchemeCb cb);
    void createScheme(const TourScheme& scheme, SchemeCb cb);
    /// 保存方案；expectedVersion 不匹配时服务端返回 409。
    void updateScheme(const TourScheme& scheme, int expectedVersion, VoidCb cb);
    void deleteScheme(qint64 id, VoidCb cb);
    /// 取可播放地址：rtsp_media → hls → flv。
    void fetchPlayUrl(const TourChannelRef& channel, UrlCb cb);
    /// 取布局列表。
    void loadLayouts(LayoutCb cb);

private:
    /// 工作线程中执行的阻塞请求：填充 success/response/error。
    using RequestFn = std::function<void(bool&, QJsonObject&, QString&)>;
    /// 回到插件线程后执行的回调。
    using DoneFn = std::function<void(bool, const QJsonObject&, const QString&)>;

    void runAsync(const RequestFn& work, const DoneFn& done);
};

} // namespace rustcam
