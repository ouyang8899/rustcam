#ifndef BRIDGE_EXPORT_H
#define BRIDGE_EXPORT_H

// 跨平台导出/导入宏
#if defined(_WIN32) || defined(_WIN64)
    #ifdef BRIDGE_BUILD_SHARED
        #define BRIDGE_EXPORT __declspec(dllexport)
    #else
        #define BRIDGE_EXPORT __declspec(dllimport)
    #endif
#elif defined(__linux__) || defined(__APPLE__)
    #ifdef BRIDGE_BUILD_SHARED
        #define BRIDGE_EXPORT __attribute__((visibility("default")))
    #else
        #define BRIDGE_EXPORT
    #endif
#else
    #define BRIDGE_EXPORT
#endif

#endif // BRIDGE_EXPORT_H
