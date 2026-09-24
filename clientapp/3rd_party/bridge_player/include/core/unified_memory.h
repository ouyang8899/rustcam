#ifndef UNIFIED_MEMORY_H
#define UNIFIED_MEMORY_H

#include <memory>
#include <iostream>
namespace BridgePlayer {
#ifdef USE_TRACKED_MEMORY
    // 使用追踪版本
#include "core/tracked_memory.h"

    template<typename T>
    using SharedPtr = tracked_shared_ptr<T>;

    template<typename T>
    using WeakPtr = tracked_weak_ptr<T>;

    template<typename T>
    using EnableSharedFromThis = tracked_enable_shared_from_this<T>;

#define MAKE_SHARED make_tracked_shared

    // 追踪配置宏
#define SET_TRACKING_LEVEL(level) TrackConfig::level = level
#define SET_TRACKING_LEAKS(enabled) TrackConfig::track_leaks = enabled
#define SET_TRACKING_WEAK(enabled) TrackConfig::track_weak_ptr = enabled
#define PRINT_TRACKING_STATUS() GlobalTracker::print_all_objects()
#define CHECK_MEMORY_LEAKS() GlobalTracker::print_all_leaks()

#else
    // 使用标准版本
    template<typename T>
    using SharedPtr = std::shared_ptr<T>;

    template<typename T>
    using WeakPtr = std::weak_ptr<T>;

    template<typename T>
    using EnableSharedFromThis = std::enable_shared_from_this<T>;

#define MAKE_SHARED std::make_shared

    // 空宏（追踪功能禁用）
#define SET_TRACKING_LEVEL(level)
#define SET_TRACKING_LEAKS(enabled)
#define SET_TRACKING_WEAK(enabled)
#define PRINT_TRACKING_STATUS()
#define CHECK_MEMORY_LEAKS()
#endif

// 辅助宏，用于条件编译
#ifdef USE_TRACKED_MEMORY
#define IF_TRACKED(expr) expr
#else
#define IF_TRACKED(expr)
#endif
}
#endif // UNIFIED_MEMORY_H
