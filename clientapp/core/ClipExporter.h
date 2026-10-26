#pragma once

#include <QObject>
#include <QString>
#include <functional>
#include "CoreExport.h"

namespace rustcam {

/// 录像片段导出：把一个录像文件按时间区间裁剪成新文件。
/// 基于 FFmpeg（libavformat）做流复制裁剪，不重新编码。
class RUSTCAM_CORE_API ClipExporter : public QObject {
    Q_OBJECT
public:
    static ClipExporter* instance();

    explicit ClipExporter(QObject* parent = nullptr);
    ~ClipExporter();

    /// 裁剪进度回调：percent 为 0~100，stage 为当前阶段说明
    using ProgressCallback = std::function<void(int percent, const QString& stage)>;

    /// 裁剪 sourcePath 的 [startMs, endMs] 区间到 outputPath。
    /// 时间是相对录像文件起点的毫秒偏移。
    /// 返回 true 表示导出成功（outputPath 已生成）。
    /// @param progress      可选，导出进度回调
    /// @param actualStartMs 可选，返回实际使用的起点（流复制时会对齐到最近关键帧）
    bool exportClip(const QString& sourcePath,
                    const QString& outputPath,
                    int64_t startMs,
                    int64_t endMs,
                    QString& error,
                    ProgressCallback progress = nullptr,
                    int64_t* actualStartMs = nullptr);

private:
    /// 精确裁剪（重新编码，起点精确到帧）
    bool exportClipReencode(const QString& sourcePath,
                             const QString& outputPath,
                             int64_t startMs,
                             int64_t endMs,
                             QString& error,
                             ProgressCallback progress,
                             int64_t* actualStartMs);
    /// 流复制裁剪（不重编码，起点落在最近的关键帧）
    bool exportClipStreamCopy(const QString& sourcePath,
                              const QString& outputPath,
                              int64_t startMs,
                              int64_t endMs,
                              QString& error,
                              ProgressCallback progress,
                              int64_t* actualStartMs);
};

}
