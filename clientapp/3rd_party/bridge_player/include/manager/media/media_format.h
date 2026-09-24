#pragma once

#include <string>
#include <sstream>
#include <iomanip>
#include <cstdint>

namespace BridgePlayer {
namespace utils {

// 格式化时间（毫秒 → HH:MM:SS 或 MM:SS）
inline std::string format_time_ms(int64_t ms) {
    if (ms < 0) return "--:--";
    
    int total_seconds = static_cast<int>(ms / 1000);
    int hours = total_seconds / 3600;
    int minutes = (total_seconds % 3600) / 60;
    int seconds = total_seconds % 60;
    
    std::ostringstream oss;
    if (hours > 0) {
        oss << hours << ":"
            << std::setw(2) << std::setfill('0') << minutes << ":"
            << std::setw(2) << std::setfill('0') << seconds;
    } else {
        oss << minutes << ":"
            << std::setw(2) << std::setfill('0') << seconds;
    }
    return oss.str();
}

// 格式化文件大小
inline std::string format_file_size(int64_t bytes) {
    std::ostringstream oss;
    if (bytes < 0) return "unknown";
    if (bytes < 1024) {
        oss << bytes << " B";
    } else if (bytes < 1024 * 1024) {
        oss << std::fixed << std::setprecision(1) 
            << (bytes / 1024.0) << " KB";
    } else if (bytes < 1024LL * 1024 * 1024) {
        oss << std::fixed << std::setprecision(1) 
            << (bytes / (1024.0 * 1024.0)) << " MB";
    } else {
        oss << std::fixed << std::setprecision(2) 
            << (bytes / (1024.0 * 1024.0 * 1024.0)) << " GB";
    }
    return oss.str();
}

// 格式化百分比
inline std::string format_percent(double value, int precision = 1) {
    std::ostringstream oss;
    oss << std::fixed << std::setprecision(precision) 
        << (value * 100.0) << "%";
    return oss.str();
}

// 格式化比特率
inline std::string format_bitrate(int64_t bps) {
    std::ostringstream oss;
    if (bps < 0) return "unknown";
    if (bps < 1000) {
        oss << bps << " bps";
    } else if (bps < 1000000) {
        oss << std::fixed << std::setprecision(1) 
            << (bps / 1000.0) << " Kbps";
    } else {
        oss << std::fixed << std::setprecision(1) 
            << (bps / 1000000.0) << " Mbps";
    }
    return oss.str();
}

// 简单字符串格式化（支持 {} 占位符）
template<typename... Args>
inline std::string format_string(const std::string& fmt, Args... args) {
    std::ostringstream oss;
    format_impl(oss, fmt, args...);
    return oss.str();
}

// 无参数版本
inline std::string format_string(const std::string& fmt) {
    return fmt;
}

// 格式化实现
template<typename T>
inline void format_impl(std::ostringstream& oss, const std::string& fmt, const T& arg) {
    auto pos = fmt.find("{}");
    if (pos != std::string::npos) {
        oss << fmt.substr(0, pos) << arg << fmt.substr(pos + 2);
    } else {
        oss << fmt;
    }
}

template<typename T, typename... Rest>
inline void format_impl(std::ostringstream& oss, const std::string& fmt, 
                       const T& arg, Rest... rest) {
    auto pos = fmt.find("{}");
    if (pos != std::string::npos) {
        oss << fmt.substr(0, pos) << arg;
        format_impl(oss, fmt.substr(pos + 2), rest...);
    } else {
        oss << fmt;
    }
}

} // namespace utils
} // namespace BridgePlayer