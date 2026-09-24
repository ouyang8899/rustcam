#pragma once

#include "manager/player_types.h"
#include "manager/media/media_error.h"
#include "manager/media/media_result.h"
#include "manager/media/media_player_config.h"



#include <string>
#include <memory>
#include <functional>
#include <atomic>
#include <mutex>
#include <thread>
#include <queue>
#include <condition_variable>
#include <map>
#include <boost/optional.hpp>
#include"manager/player_base.h"
#include"platform/native_window_handle.h"
#include"manager/player_types.h"
#include "core/unified_memory.h"
#include"core/bridge_export.h"
namespace BridgePlayer {
    class PlayerManager;

    using MediaEventCallback = std::function<void(MediaPlayerEvent, const MediaPlayerStatus&)>;
    using MediaVideoFrameCallback = std::function<void(std::shared_ptr<MediaFrameData>,bool)>;
    using MediaAudioFrameCallback = std::function<void(std::shared_ptr<MediaFrameData>)>;
    class BRIDGE_EXPORT MediaPlayer : public PlayerBase{
    public:
       /* struct Stats {
            int64_t video_frames_decoded{ 0 };
            int64_t audio_frames_decoded{ 0 };
            int64_t video_frames_dropped{ 0 };
            int64_t video_frames_rendered{ 0 };
            int64_t audio_frames_rendered{ 0 };
            int64_t video_queue_size{ 0 };
            int64_t audio_queue_size{ 0 };
            int64_t buffered_bytes{ 0 };
        };*/
        using Ptr = SharedPtr<MediaPlayer>;
        using WPtr = WeakPtr<MediaPlayer>;
    protected:
        explicit MediaPlayer(const uint64_t& id,
            const uint64_t& render_target_id,
            NativeWindowHandle handle = nullptr);
        MediaPlayer() = delete;
        MediaPlayer(const MediaPlayer& other) = delete;
        MediaPlayer(MediaPlayer&& other) = delete;
    public:
        SharedPtr<MediaPlayer> child_shared_from_this() {
            return std::static_pointer_cast<MediaPlayer>(PlayerBase::shared_from_this());
        }
        ~MediaPlayer();
        Result<void> initialize(const MediaPlayerConfig& config = MediaPlayerConfig());
    public:
        virtual bool start(const std::string& url) override;
        virtual void in_context(Task task) override;
        virtual void set_audio_enabled(bool enabled) override;
        virtual bool re_start() override;
        virtual void stop() override;
        virtual bool is_playing() const;
        const Stats& get_stats()  override;
    protected:

        Result<void> pause();
        Result<void> seek(int64_t position_ms);
        Result<void> seek_relative(int64_t delta_ms);

        void set_speed(double speed);
        void set_volume(double volume);
        void set_mute(bool mute);
        void set_sync_mode(int mode);

        Result<void> select_video_track(int index);
        Result<void> select_audio_track(int index);

        boost::optional<MediaInfo> get_media_info() const;
        bool can_seek() const;
        bool can_show_duration() const;
        bool is_live_stream() const;

        void set_event_callback(MediaEventCallback callback);
        //void set_video_frame_callback(MediaFrameCallback callback);
        //void set_audio_frame_callback(MediaFrameCallback callback);
        const std::unique_ptr<SDLAudioPlayer>& get_audio_player() { return this->audio_player_; };
    private:
        class Impl;
        std::unique_ptr<Impl> impl_;

        friend class PlayerManager;
    };

} // namespace BridgePlayer