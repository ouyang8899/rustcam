#pragma once

#include <QJsonObject>
#include <QString>

namespace rustcam {

/// 本地审计日志：记录启停、翻页、失败通道与方案变更。
class TourAuditLog {
public:
    /// 写一条审计记录，格式：ISO8601 时间 事件名 JSON 明细。
    static void log(const QString& event, const QJsonObject& detail = QJsonObject());

    /// 日志文件路径（logs/video_tour/video_tour.log）。
    static QString logFilePath();
};

} // namespace rustcam
