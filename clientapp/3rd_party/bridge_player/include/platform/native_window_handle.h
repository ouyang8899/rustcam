// NativeWindowHandle.h
#pragma once

#include <cstdint>
#include <cstring>
#include <cstdlib>
#include <functional>
#include"core/bridge_export.h"
#ifdef _WIN32
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#elif defined(__linux__)
#include <wayland-client.h>
#include <X11/Xlib.h>
#elif defined(OHOS_PLATFORM) || defined(__OHOS__)
#include <native_window/external_window.h>
#endif

namespace BridgePlayer {

    class BRIDGE_EXPORT NativeWindowHandle {
    public:
        // 平台类型枚举
        enum class PlatformType : uint8_t {
            Unknown,
            Windows,
            X11,
            Wayland,
            HarmonyOS
        };
    public:
        // 哈希仿函数（用于 unordered_map/unordered_set）
        struct Hash {
            size_t operator()(const NativeWindowHandle& handle) const noexcept {
                if (!handle.is_valid()) return 0;
                return std::hash<uintptr_t>{}(handle.get_int_handle());
            }
        };

        // ============ 构造函数 ============
        NativeWindowHandle() = default;
        NativeWindowHandle(std::nullptr_t) : NativeWindowHandle() {}

#ifdef _WIN32
        explicit NativeWindowHandle(HWND hwnd);
#endif

#ifdef __linux__
        // X11 构造函数
        NativeWindowHandle(Display* display, ::Window window);
        explicit NativeWindowHandle(::Window window);

        // Wayland 构造函数
        explicit NativeWindowHandle(wl_surface* surface);

        static const char* get_display_server();
#endif

#ifdef OHOS_PLATFORM
        explicit NativeWindowHandle(OHNativeWindow* handle);
#endif

        // ============ 拷贝/移动语义（显式实现） ============
        NativeWindowHandle(const NativeWindowHandle& other);
        NativeWindowHandle& operator=(const NativeWindowHandle& other);
        NativeWindowHandle(NativeWindowHandle&& other) noexcept;
        NativeWindowHandle& operator=(NativeWindowHandle&& other) noexcept;

        // ============ 赋值运算符 ============
        NativeWindowHandle& operator=(std::nullptr_t) {
            clear();
            return *this;
        }

        // ============ 查询方法 ============
        bool is_valid() const { return valid_; }
        void* get_raw_handle() const;
        uintptr_t get_int_handle() const;
        const char* get_platform_name() const;
        PlatformType get_platform_type();
        // ============ 类型转换 ============
        explicit operator bool() const { return valid_; }

        // ============ 比较运算符 ============
        bool operator==(const NativeWindowHandle& other) const;
        bool operator!=(const NativeWindowHandle& other) const;
        bool operator==(std::nullptr_t) const { return !valid_; }
        bool operator!=(std::nullptr_t) const { return valid_; }
        bool operator<(const NativeWindowHandle& other) const;

        ~NativeWindowHandle();

    private:
        void clear();
        void copy_from(const NativeWindowHandle& other);



        // 句柄存储结构
        struct HandleStorage {
            PlatformType type = PlatformType::Unknown;
            union {
                void* ptr_;
                uintptr_t value_;
#ifdef _WIN32
                HWND hwnd_;
#endif
#ifdef __linux__
                struct {
                    Display* display_;
                    ::Window window_;
                } x11;
                wl_surface* wayland_surface_;
#endif
#ifdef OHOS_PLATFORM
                OHNativeWindow* native_window_;
#endif
            } data;

            HandleStorage() : data{ nullptr } {}
        };

        HandleStorage handle_;
        bool valid_ = false;

#ifdef __linux__
        bool owns_display_ = false;  // 是否拥有 Display 连接（需要关闭）
#endif
    };

} // namespace BridgePlayer

// ============ std::hash 特化（支持 unordered_map） ============
namespace std {
    template<>
    struct hash<BridgePlayer::NativeWindowHandle> {
        size_t operator()(const BridgePlayer::NativeWindowHandle& handle) const noexcept {
            if (!handle.is_valid()) return 0;
            return hash<uintptr_t>{}(handle.get_int_handle());
        }
    };
}