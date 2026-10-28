#pragma once
#include <cstdint>
#include <string>
#include <chrono>
#include <functional>
#include<memory>
#include<vector>
#include<atomic>
#include"platform/native_window_handle.h"
extern "C" {
#include <libavformat/avformat.h>
#include <libavcodec/avcodec.h>
#include <libavutil/error.h>
#include <libavutil/mem.h>
#include <libavutil/avutil.h>
}
#include<stdint.h>
#include<cinttypes>
namespace BridgePlayer{
using std::uint64_t;  // 将 std::uint64_t 引入当前命名空间
   // 时间类型定义
using Microseconds = std::chrono::microseconds;
using Milliseconds = std::chrono::milliseconds;
using Seconds = std::chrono::seconds;
using TimePoint = std::chrono::steady_clock::time_point;
using PlayerIdType = ::uint64_t;

// 缩放质量
enum class ScaleQuality
{
    FAST = 0,     // 最近邻（最快，质量最低）
    BILINEAR = 1, // 双线性（默认，平衡）
    BICUBIC = 2   // 双三次（最慢，质量最高）
};
enum class PlayerState
{
    IDLE,
    CONNECTING,
    CONNECTED,
    INITIALIZING,
    BUFFERING,
    READY,
    PLAYING,
    SEEKING,
    PAUSED,
    STOPPED,
    ERROR_
};
enum class RendererType {
    OPENGL,
    D3D11,
    D3D12,
    VULKAN
};
typedef struct PlayerLayout
{
    int left;
    int top;
    int width;
    int height;
} PlayerLayout;
enum class VideoMirrorMode: uint8_t {
    None,       // 不镜像
    Horizontal=0x01, // 水平镜像（左右翻转）
    Vertical=0x02,   // 垂直镜像（上下翻转）
    Both=0x03        // 水平+垂直
};
enum class VideoDecodeType:uint16_t {
    SOFTWARE = 1,
    HARDWARE = 2,
    AUTO = 3,
};
typedef struct Color {
    int r=0;
    int g=0;
    int b=0;
    int a=255;
}Color;

enum class Align: uint64_t {
    none = 0,
    top = 1 << 0,
    left = 1 << 1,
    right = 1 << 2,
    bottom = 1 << 3,
    centerH = 1 << 4,  // 显式水平居中
    centerV = 1 << 5,  // 显式垂直居中
};
inline  Align operator|(Align a, Align b) {
    return static_cast<Align>(static_cast<uint64_t>(a) | static_cast<uint64_t>(b));
}
inline  Align operator&(Align a, Align b) {
    return static_cast<Align>(static_cast<uint64_t>(a) & static_cast<uint64_t>(b));
}
inline  bool has(Align a, Align b) {
    return (static_cast<uint64_t>(a) & static_cast<uint64_t>(b)) != 0;
}
typedef struct Margin {
    int left = 0;
    int top = 0;
    int right = 0;
    int bottom = 0;
} Margin;
typedef struct Rect2D {
    int left = 0;
    int top = 0;
    int width = 0;
    int height = 0;

    static Rect2D alignRect(const Rect2D& window,
        Rect2D control,
        Align align,
        const Margin& margin = {})
    {
        const int winL = window.left;
        const int winT = window.top;
        const int winR = window.left + window.width;
        const int winB = window.top + window.height;

        const bool alignTop = has(align, Align::top);
        const bool alignBottom = has(align, Align::bottom);
        const bool alignLeft = has(align, Align::left);
        const bool alignRight = has(align, Align::right);

        // ---- 水平 ----
        // 宽高固定，left|right 同时指定时按优先级选一个
        // 这里优先级：left > right
        if (alignLeft) {
            control.left = winL + margin.left;
        }
        else if (alignRight) {
            control.left = winR - margin.right - control.width;
        }
        else {
            control.left = winL + (window.width - control.width) / 2;
        }

        // ---- 垂直 ----
        // 优先级：top > bottom
        if (alignTop) {
            control.top = winT + margin.top;
        }
        else if (alignBottom) {
            control.top = winB - margin.bottom - control.height;
        }
        else {
            control.top = winT + (window.height - control.height) / 2;
        }

        return control;
    }
} Rect2D;

typedef struct Point2D {
    using Ptr = std::shared_ptr<Point2D>;
    int x;
    int y;
}Point2D;

typedef struct ZoomData {
    using Ptr = std::shared_ptr<ZoomData>;
    Point2D::Ptr center{nullptr};
    float scale{ 1.0 };
    bool enable{false};
}ZoomData;
typedef struct VideoNameData {
    using Ptr = std::shared_ptr<VideoNameData>;
    std::string name;
    Color color;
    Color background;
    int font_size;
    uint64_t align;
    bool mirror_roate_with_video{ true };
}VideoNameData;

typedef struct VideoNameDisplay {
    using Ptr = std::shared_ptr<VideoNameDisplay>;
    VideoNameData::Ptr data{nullptr};
    bool visible{ false };
}VideoNameDisplay;
typedef struct VideoWatermarkData {
    using Ptr = std::shared_ptr<VideoWatermarkData>;
    std::string title;
    Color color;
    int font_size;
    int degree{ 30 };
    bool mirror_roate_with_video{ true };
}VideoWaterMarkData;
typedef struct VideoWatermarkDisplay {
    using Ptr = std::shared_ptr<VideoWatermarkDisplay>;
    VideoWatermarkData::Ptr data{nullptr};
    bool visible{ false };
} VideoWatermarkDisplay;

typedef struct VideoLocaleTimeData {
    using Ptr = std::shared_ptr<VideoLocaleTimeData>;
    std::function <std::string(void)> get_time_fun{ nullptr };
    Color color;
    int font_size;
    uint64_t align;
    bool mirror_roate_with_video{ true };
} VideoLocaleTimeData;

typedef struct VideoLocaleTimeDisplay {
    using Ptr = std::shared_ptr<VideoLocaleTimeDisplay>;
    VideoLocaleTimeData::Ptr data{nullptr};
    bool visible{ false };
}VideoLocaleTimeDisplay;

typedef struct VideoMediaRateData {
    using Ptr = std::shared_ptr<VideoMediaRateData>;
    Color color;
    int font_size;
    uint64_t align;
    bool mirror_roate_with_video{ true };

}VideoMediaRateData;

typedef struct VideoMediaRateDisplay {
    using Ptr = std::shared_ptr<VideoMediaRateDisplay>;
    VideoMediaRateData::Ptr data{ nullptr };
    bool visible{ false };
} VideoMediaRateDisplay;
typedef struct PlayerData
{
    std::uint64_t id_{0};
    PlayerState state{PlayerState::IDLE};
    PlayerLayout layout{0, 0, 0, 0};
    std::string url;
} PlayerData;
typedef   struct RenderTargetData {
        NativeWindowHandle handle{ nullptr };
        uint64_t target_id;
        int width{ 800 };
        int height{ 600 };
    } RenderTargetData;
using Task = std::function<void()>;
struct RenderParamValue {
    float default_value;
    float value;
    bool use_default{ true };
    float get_value() {
        return use_default ? default_value : value;
    }
};
enum class MediaPlayerEvent {
    STATE_CHANGED,
    PROGRESS_UPDATED,
    METADATA_READY,
    BUFFERING_START,
    BUFFERING_END,
    SEEK_COMPLETED,
    DOWNLOAD_PROGRESS,
    DOWNLOAD_COMPLETE,
    ERROR_OCCURRED,
    PLAYBACK_COMPLETE
};


// PTS/DTS时间戳
struct MediaTimestamp {
    int64_t pts{AV_NOPTS_VALUE};  // Presentation Time Stamp
    int64_t dts{AV_NOPTS_VALUE};  // Decode Time Stamp
    int64_t duration{0};          // 帧持续时间

    bool has_pts() const { return pts != AV_NOPTS_VALUE; }
    bool has_dts() const { return dts != AV_NOPTS_VALUE; }

    int64_t best_effort_timestamp() const {
        return has_pts() ? pts : (has_dts() ? dts : 0);
    }
};

// 媒体源类型
enum class MediaSourceType {
    LOCAL_FILE,              // 本地文件
    NETWORK_WITH_MOOV,       // 网络文件带moov头
    NETWORK_WITHOUT_MOOV,    // 网络文件无moov头
    LIVE_STREAM,             // 直播流（RTMP/RTSP等）
    UNKNOWN
};

// 播放策略
enum class MediaPlayStrategy {
    IMMEDIATE,          // 立即播放
    BUFFER_AND_PLAY,    // 缓冲后播放
    WAIT_FOR_METADATA,  // 等待元数据
    STREAM_MODE         // 流模式
};

// 播放状态
//enum class MediaPlayerState {
//    IDLE,
//    INITIALIZING,
//    READY,
//    PLAYING,
//    PAUSED,
//    SEEKING,
//    BUFFERING,
//    COMPLETED,
//    ERROR_
//};

// 播放模式
enum class MediaPlaybackMode {
    FULL_FEATURED,    // 完整功能
    PROGRESSIVE,      // 渐进式
    STREAMING,        // 流式
    PREVIEW           // 预览
};

// 缓冲策略
enum class MediaBufferStrategy {
    MINIMAL,      // 最小缓冲
    NORMAL,       // 正常缓冲
    AGGRESSIVE    // 激进缓冲
};

// 媒体类型
enum class MediaType {
    UNKNOWN,
    VIDEO,
    AUDIO,
    SUBTITLE
};

// 音视频同步模式
//enum class MediaSyncMode {
//    AUDIO_MASTER,
//    VIDEO_MASTER,
//    EXTERNAL_CLOCK
//};

// 帧数据
struct MediaFrameData {
    AVFrame* frame{nullptr};
    MediaTimestamp timestamp;
    AVRational time_base{1, 1};
    MediaType type{MediaType::UNKNOWN};
    int stream_index{-1};
    bool key_frame{false};
    int64_t pts_ms() const {
        if (!timestamp.has_pts()) return 0;
        return av_rescale_q(timestamp.pts, time_base, {1, 1000});
    }

    int64_t duration_ms() const {
        return av_rescale_q(timestamp.duration, time_base, {1, 1000});
    }
};

// 播放器状态信息（用于UI更新）
struct MediaPlayerStatus {
    PlayerState state{ PlayerState::IDLE};
    MediaPlaybackMode mode{MediaPlaybackMode::FULL_FEATURED};

    // 时间信息
    int64_t current_time_ms{0};      // 当前播放时间
    int64_t total_duration_ms{-1};   // 总时长（-1表示未知）
    int64_t buffered_duration_ms{0}; // 已缓冲时长

    // 进度信息
    double playback_progress{0.0};   // 播放进度 0.0-1.0
    double download_progress{0.0};   // 下载进度 0.0-1.0
    double buffer_health{1.0};       // 缓冲健康度 0.0-1.0

    // 字节信息
    int64_t bytes_downloaded{0};
    int64_t total_bytes{-1};

    // 速度信息
    double speed{1.0};
    double volume{1.0};

    // 能力标志
    bool can_seek{false};
    bool can_show_duration{false};
    bool can_show_progress{false};
    bool is_buffering{false};
    bool is_complete{false};

    // 文本描述
    std::string status_text;
    std::string time_text;
};

// 流信息
struct MediaStreamInfo {
    int index{-1};
    MediaType type{MediaType::UNKNOWN};
    std::string codec_name;
    std::string language;
    bool enabled{true};

    // 视频特定
    int width{0};
    int height{0};
    double frame_rate{0.0};

    // 音频特定
    int sample_rate{0};
    int channels{0};
};

// 媒体文件信息
struct MediaInfo {
    std::string url;
    std::string format_name;
    int64_t duration_ms{-1};
    int64_t file_size{0};
    int64_t bitrate{0};

    std::vector<MediaStreamInfo> streams;
    std::vector<MediaStreamInfo> video_streams;
    std::vector<MediaStreamInfo> audio_streams;
    std::vector<MediaStreamInfo> subtitle_streams;

    MediaSourceType source_type{ MediaSourceType::UNKNOWN};
    bool has_moov{false};
    bool is_seekable{false};

    std::string title;
    std::string artist;
    std::string album;
};
}