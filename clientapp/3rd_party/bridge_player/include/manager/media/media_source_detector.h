#pragma once
#include "manager/player_types.h"
#include "manager/media/media_error.h"
#include"manager/media/media_result.h"
#include <string>
#include <memory>
namespace BridgePlayer{
// 源检测结果
struct MediaSourceDetectionResult {
    MediaSourceType type{ MediaSourceType::UNKNOWN};
    MediaPlayStrategy strategy{ MediaPlayStrategy::IMMEDIATE};
    MediaBufferStrategy buffer_strategy{ MediaBufferStrategy::NORMAL};

    // 能力描述
    bool can_seek{false};
    bool has_duration{false};
    bool has_byte_range{false};
    bool needs_download{false};

    // 网络信息
    int64_t content_length{-1};     // 内容长度（字节）
    int64_t estimated_bitrate{0};   // 估算比特率
    int64_t estimated_duration{-1}; // 估算时长（毫秒）

    // 下载信息
    int64_t moov_position{-1};      // moov位置（字节偏移）
    bool moov_at_end{false};        // moov是否在文件末尾

    // 描述文本
    std::string description;
};

class MediaSourceDetector {
public:
    MediaSourceDetector();
    ~MediaSourceDetector();

    // 检测源类型
    Result<MediaSourceDetectionResult> detect(const std::string& url);
    Result<MediaSourceDetectionResult> detectLight(const std::string& url);
    // 检查是否为网络URL
    static bool isNetworkURL(const std::string& url);

    // 检查是否为直播流
    static bool isLiveStream(const std::string& url);

    // 获取最佳缓冲策略
    static MediaBufferStrategy getOptimalBufferStrategy(
        const MediaSourceDetectionResult& detection);

private:
    class Impl;
    std::unique_ptr<Impl> impl_;

    // 探测HTTP能力
    Result<MediaSourceDetectionResult> probeHTTP(const std::string& url);

    // 检查moov原子位置
    Result<bool> checkMoovPosition(const std::string& url,
                                   MediaSourceDetectionResult& result);

    // 分析本地文件
    Result<MediaSourceDetectionResult> analyzeLocalFile(const std::string& path);
};


}