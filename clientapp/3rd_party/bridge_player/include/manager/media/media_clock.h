#pragma once

#include <cstdint>
#include <memory>
#include <mutex>
#include <atomic>
#include <functional>

namespace BridgePlayer {

    enum class MediaSyncMode {
        AUDIO_MASTER,      // 音频为主时钟（从外部音频播放器获取）
        VIDEO_MASTER,      // 视频为主时钟（视频线程更新）
        EXTERNAL_CLOCK     // 外部时钟（调用方 setTime）
    };

    // 音频时钟获取回调（AUDIO_MASTER 模式下使用）
    using AudioClockGetter = std::function<int64_t()>;

    class MediaClock {
    public:
        explicit MediaClock(MediaSyncMode mode = MediaSyncMode::AUDIO_MASTER);
        ~MediaClock();

        // ========== 生命周期 ==========
        void start(int64_t base_time_us);
        void stop();
        void pause();
        void resume();
        void reset();

        // ========== 时间设置 ==========
        void setTime(int64_t time_us);
        void setSpeed(double speed);
        void setPaused(bool paused);

        // ========== 时间查询 ==========
        int64_t getTime() const;       // 媒体时钟时间（微秒），受 speed 影响
        int64_t getTimeMs() const;
        int64_t getMasterTime() const; // 主时钟时间（微秒），实际播放基准
        double getSpeed() const;
        bool isPaused() const;

        // ========== 同步模式 ==========
        void setSyncMode(MediaSyncMode mode);
        MediaSyncMode getSyncMode() const;

        // 设置音频时钟获取回调（AUDIO_MASTER 模式下必须设置）
        void setAudioClockGetter(AudioClockGetter getter);

        // ========== 主时钟更新（VIDEO_MASTER / EXTERNAL 模式下调用）==========
        void updateVideoMaster(int64_t pts_us);

        // ========== 帧同步辅助 ==========
        /// 计算帧的显示延迟（微秒）
        /// 返回 > 0：需要等待的时间；< 0：已经超时，考虑丢帧
        int64_t calculateDelay(int64_t pts_us) const;

        /// 判断是否到了播放时间（误差在 threshold_us 内）
        bool isTimeToPlay(int64_t pts_us, int64_t threshold_us = 10000) const;

        // ========== Serial 机制（seek 后丢弃旧帧）==========
        int64_t getSerial() const;
        int64_t incrementSerial();

    private:
        class Impl;
        std::unique_ptr<Impl> impl_;
    };

} // namespace BridgePlayer