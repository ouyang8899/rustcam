#pragma once
#include "manager/player_types.h"
//#include "private/media/media_clock.h"  // 添加这一行！
#include <cstddef>
#include <string>

namespace BridgePlayer {

    struct MediaPlayerConfig {
        // 缓冲配置
        struct MediaBuffer {
            size_t video_queue_size{ 15 };
            size_t audio_queue_size{ 50 };
            size_t packet_queue_size{ 100 };
            int64_t min_buffer_ms{ 500 };
            int64_t max_buffer_ms{ 5000 };
            MediaBufferStrategy strategy{ MediaBufferStrategy::NORMAL };
        } buffer;

        // 解码配置
        struct MediaDecode {
            int thread_count{ 0 };
            bool hardware_acceleration{ false };
            bool low_latency{ false };
            bool drop_late_frames{ true };
        } decode;

        // 网络配置
        struct MediaNetwork {
            int timeout_ms{ 10000 };
            int connect_timeout_ms{ 5000 };
            int max_retries{ 3 };
            bool follow_redirects{ true };
            std::string user_agent{ "MediaPlayer/1.0" };
            std::string proxy;
        } network;

        // 播放配置
        struct MediaPlayback {
            double speed{ 1.0 };
            double volume{ 1.0 };
            bool muted{ false };
            int sync_mode{ 0 };  // 现在 MediaSyncMode 已定义
            int64_t start_position_ms{ 0 };
            bool auto_play{ true };
            bool loop{ false };
        } playback;

        // 渲染配置
        struct MediaRender {
            int video_width{ 0 };
            int video_height{ 0 };
            int fps{ 0 };
            bool vsync{ true };
        } render;

        static MediaPlayerConfig forLocalFile() {
            MediaPlayerConfig config;
            config.buffer.strategy = MediaBufferStrategy::MINIMAL;
            config.buffer.min_buffer_ms = 100;
            config.buffer.video_queue_size = 10;
            config.buffer.audio_queue_size = 20;
            return config;
        }

        static MediaPlayerConfig forNetworkStream() {
            MediaPlayerConfig config;
            config.buffer.strategy = MediaBufferStrategy::NORMAL;
            config.buffer.min_buffer_ms = 1000;
            config.buffer.max_buffer_ms = 5000;
            config.buffer.video_queue_size = 15;
            config.buffer.audio_queue_size = 50;
            config.network.timeout_ms = 15000;
            return config;
        }

        static MediaPlayerConfig forLiveStream() {
            MediaPlayerConfig config;
            config.buffer.strategy = MediaBufferStrategy::MINIMAL;
            config.buffer.min_buffer_ms = 100;
            config.buffer.max_buffer_ms = 1000;
            config.buffer.video_queue_size = 5;
            config.buffer.audio_queue_size = 10;
            config.decode.low_latency = true;
            config.decode.drop_late_frames = true;
            config.network.timeout_ms = 30000;
            return config;
        }

        static MediaPlayerConfig forSlowNetwork() {
            MediaPlayerConfig config;
            config.buffer.strategy = MediaBufferStrategy::AGGRESSIVE;
            config.buffer.min_buffer_ms = 3000;
            config.buffer.max_buffer_ms = 10000;
            config.buffer.video_queue_size = 30;
            config.buffer.audio_queue_size = 100;
            config.network.timeout_ms = 30000;
            config.network.max_retries = 5;
            return config;
        }
    };
}