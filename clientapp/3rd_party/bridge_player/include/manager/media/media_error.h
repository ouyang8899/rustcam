#pragma once

#include <system_error>
#include <string>
#include <boost/assert/source_location.hpp>

namespace BridgePlayer {

    // 在 MediaErrorCode 枚举中添加：
    enum class MediaErrorCode {
        SUCCESS = 0,

        // 文件错误 (1000-1999)
        FILE_NOT_FOUND = 1000,
        FILE_ACCESS_DENIED = 1001,
        FILE_FORMAT_NOT_SUPPORTED = 1002,
        FILE_CORRUPTED = 1003,

        // 网络错误 (2000-2999)
        NETWORK_UNREACHABLE = 2000,
        NETWORK_TIMEOUT = 2001,
        NETWORK_DNS_ERROR = 2002,
        NETWORK_SSL_ERROR = 2003,
        NETWORK_BANDWIDTH_LOW = 2004,

        // 解码错误 (3000-3999)
        DECODER_NOT_FOUND = 3000,
        DECODER_INIT_FAILED = 3001,
        DECODER_HARDWARE_FAILED = 3002,
        DECODE_FAILED = 3003,
        DECODE_EOF = 3004,        // 添加这个
        DECODE_FLUSHED = 3005,    // 添加这个

        // 播放器错误 (4000-4999)
        PLAYER_NOT_INITIALIZED = 4000,
        PLAYER_ALREADY_PLAYING = 4001,
        PLAYER_SEEK_FAILED = 4002,
        PLAYER_OUT_OF_SYNC = 4003,

        // 资源错误 (5000-5999)
        OUT_OF_MEMORY = 5000,
        QUEUE_OVERFLOW = 5001,

        // 参数错误 (6000-6999)
        INVALID_PARAMETER = 6000,
        NOT_SUPPORTED = 6001,

        // 内部错误 (9000-9999)
        INTERNAL_ERROR = 9000
    };

    // 错误类别
    class MediaErrorCategory : public std::error_category {
    public:
        const char* name() const noexcept override {
            return "BridgePlayer.Media";
        }

        std::string message(int ev) const override {
            switch (static_cast<MediaErrorCode>(ev)) {
            case MediaErrorCode::SUCCESS: return "Success";
            case MediaErrorCode::FILE_NOT_FOUND: return "File not found";
            case MediaErrorCode::FILE_ACCESS_DENIED: return "File access denied";
            case MediaErrorCode::FILE_FORMAT_NOT_SUPPORTED: return "File format not supported";
            case MediaErrorCode::FILE_CORRUPTED: return "File corrupted";
            case MediaErrorCode::NETWORK_UNREACHABLE: return "Network unreachable";
            case MediaErrorCode::NETWORK_TIMEOUT: return "Network timeout";
            case MediaErrorCode::NETWORK_DNS_ERROR: return "DNS error";
            case MediaErrorCode::NETWORK_SSL_ERROR: return "SSL error";
            case MediaErrorCode::NETWORK_BANDWIDTH_LOW: return "Network bandwidth insufficient";
            case MediaErrorCode::DECODER_NOT_FOUND: return "Decoder not found";
            case MediaErrorCode::DECODER_INIT_FAILED: return "Decoder initialization failed";
            case MediaErrorCode::DECODER_HARDWARE_FAILED: return "Hardware decoder failed";
            case MediaErrorCode::DECODE_FAILED: return "Decode failed";
            case MediaErrorCode::PLAYER_NOT_INITIALIZED: return "Player not initialized";
            case MediaErrorCode::PLAYER_ALREADY_PLAYING: return "Player already playing";
            case MediaErrorCode::PLAYER_SEEK_FAILED: return "Seek failed";
            case MediaErrorCode::PLAYER_OUT_OF_SYNC: return "Audio/video out of sync";
            case MediaErrorCode::OUT_OF_MEMORY: return "Out of memory";
            case MediaErrorCode::QUEUE_OVERFLOW: return "Queue overflow";
            case MediaErrorCode::INVALID_PARAMETER: return "Invalid parameter";
            case MediaErrorCode::NOT_SUPPORTED: return "Not supported";
            case MediaErrorCode::INTERNAL_ERROR: return "Internal error";
            default: return "Unknown error";
            }
        }

        // 获取单例
        static const MediaErrorCategory& instance() {
            static MediaErrorCategory category;
            return category;
        }
    };

    // 获取error_category的全局函数
    inline const std::error_category& media_error_category() {
        return MediaErrorCategory::instance();
    }

    // 创建error_code
    inline std::error_code make_error_code(MediaErrorCode e) {
        return std::error_code(static_cast<int>(e), media_error_category());
    }

    // 创建error_condition
    inline std::error_condition make_error_condition(MediaErrorCode e) {
        return std::error_condition(static_cast<int>(e), media_error_category());
    }

    // ============================================================
    // MediaError 结构体
    // ============================================================
    struct MediaError {
        MediaErrorCode code{MediaErrorCode::SUCCESS};
        std::string message;
        std::string detail;
        std::string file;
        int line{0};
        std::string function;

        // 默认构造函数
        MediaError() = default;

        // 主要构造函数
        MediaError(MediaErrorCode c,
                   const std::string& msg = "",
                   const std::string& det = "",
                   const char* f = "",
                   int l = 0,
                   const char* func = "")
            : code(c)
            , message(msg)
            , detail(det)
            , file(f ? f : "")
            , line(l)
            , function(func ? func : "") {}

        // 从错误码构造
        explicit MediaError(MediaErrorCode c)
            : code(c)
            , message(MediaErrorCategory::instance().message(static_cast<int>(c))) {}

        // 从std::error_code构造
        explicit MediaError(const std::error_code& ec)
            : code(static_cast<MediaErrorCode>(ec.value()))
            , message(ec.message()) {}

        // 状态检查
        bool is_success() const noexcept {
            return code == MediaErrorCode::SUCCESS;
        }

        bool is_error() const noexcept {
            return code != MediaErrorCode::SUCCESS;
        }

        explicit operator bool() const noexcept {
            return is_error();
        }

        bool operator!() const noexcept {
            return is_success();
        }

        // 相等比较
        bool operator==(const MediaError& other) const noexcept {
            return code == other.code;
        }

        bool operator!=(const MediaError& other) const noexcept {
            return code != other.code;
        }

        bool operator==(MediaErrorCode c) const noexcept {
            return code == c;
        }

        bool operator!=(MediaErrorCode c) const noexcept {
            return code != c;
        }

        // 转换为std::error_code
        std::error_code to_error_code() const {
            return make_error_code(code);
        }

        // 获取完整错误消息
        std::string full_message() const {
            std::string result = message;

            if (!detail.empty()) {
                result += " (" + detail + ")";
            }

            if (!file.empty()) {
                result += " [" + file;
                if (line > 0) {
                    result += ":" + std::to_string(line);
                }
                if (!function.empty()) {
                    result += " in " + function;
                }
                result += "]";
            }

            return result;
        }

        // 静态工厂方法
        static MediaError success() {
            return MediaError();
        }

        static MediaError create(MediaErrorCode code,
                                const std::string& msg = "",
                                const char* file = "",
                                int line = 0,
                                const char* func = "") {
            return MediaError(code, msg, "", file, line, func);
        }

        static MediaError from_error_code(const std::error_code& ec) {
            return MediaError(ec);
        }
    };

} // namespace BridgePlayer

// ============================================================
// std命名空间特化
// ============================================================
namespace std {
    template<>
    struct is_error_code_enum<BridgePlayer::MediaErrorCode> : true_type {};

    template<>
    struct is_error_condition_enum<BridgePlayer::MediaErrorCode> : true_type {};
}

// ============================================================
// 便利宏
// ============================================================

// 创建简单错误
#define MAKE_ERROR(code, msg) \
    BridgePlayer::MediaError(code, msg, "", __FILE__, __LINE__, __FUNCTION__)

// 创建带详情的错误
#define MAKE_ERROR_DETAIL(code, msg, detail) \
    BridgePlayer::MediaError(code, msg, detail, __FILE__, __LINE__, __FUNCTION__)

// 仅使用错误码创建错误
#define MAKE_ERROR_CODE(code) \
    BridgePlayer::MediaError(code, "", "", __FILE__, __LINE__, __FUNCTION__)

// 成功
#define SUCCESS_ERROR() \
    BridgePlayer::MediaError::success()