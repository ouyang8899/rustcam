#pragma once
#include "core/bridge_export.h"
#include <boost/asio.hpp>
#include <memory>
#include <string>
#include <atomic>
#include <functional>
#include <map>
#include<unordered_map>
#include <mutex>
#include<stdint.h>
#include<cinttypes>
#include "core/unified_memory.h"
#include "platform/native_window_handle.h"
#include"manager/player_types.h"
#include"core/player_code.h"
extern "C"
{
    #include <libavutil/frame.h>
    #include <libavcodec/codec_id.h>
}
namespace BridgePlayer{
    class TaskRenderBase;
    class SDLAudioPlayer;

    class BRIDGE_EXPORT PlayerBase : public EnableSharedFromThis<PlayerBase> {
    public:
        struct Stats
        {
            std::atomic<uint64_t> frames_decoded{ 0 };
            std::atomic<uint64_t> frames_converted{ 0 };
            std::atomic<uint64_t> frames_dropped{ 0 };
            std::atomic<uint64_t> frames_rendererd{ 0 };
            std::atomic<uint64_t> frames_broadcast{ 0 };
            std::atomic<uint64_t> audio_frames_decoded{ 0 };
            std::atomic<uint64_t> audio_frames_rendered{ 0 };
            std::atomic<uint64_t> video_queue_size{ 0 };
            std::atomic<uint64_t> audio_queue_size{ 0 };
            std::atomic<uint64_t> fallback_count{ 0 };
            std::atomic<uint64_t> total_conversion_us{ 0 };
            std::atomic<uint64_t> audio_frames_converted{ 0 };
            std::atomic<uint64_t> conversion_errors{ 0 };
            std::atomic<uint64_t> buffered_bytes{ 0 };
            std::atomic<uint64_t> resize_count{ 0 }; // 分辨率调整次数
            std::atomic<uint64_t> errors{ 0 };
        };

    public:
        using Ptr = SharedPtr<PlayerBase>;
        using WPtr = WeakPtr<PlayerBase>;
        using ID = uint64_t;
    public:
        // 宽高变化回调
        using ResizeCallback = std::function<void(uint64_t, int, int)>;
        using FrameSender = std::function<void(const uint64_t& player_id,
            const std::vector<uint8_t>& yuv_data,
            int width, int height,
            int64_t pts)>;
        using RGBFrameSender = std::function<void(const uint64_t& player_id,
            const uint64_t& render_target_id,
            std::vector<uint8_t>& rgb_data,
            int width, int height,
            int64_t pts,int color_type)>;
        using MessageSender = std::function<void(const std::string& message)>;
        using PlayStatusCallback = std::function<void(const PlayerBase::WPtr& player, PlayCode code)>;
    public:
        PlayerBase(const std::uint64_t& id, const uint64_t render_target_id, NativeWindowHandle handler);
        virtual ~PlayerBase();
 /*       template <typename T> SharedPtr<T> child_shared_from_this() {
            return std::static_pointer_cast<T>(std::enable_shared_from_this<T>::shared_from_this());
        }*/
        SharedPtr<PlayerBase> shared_from_this_base() {
            return shared_from_this();
        }

        // 为特定播放器设置图像参数
        void set_image_params( float brightness, float contrast,
            float saturation, float gamma);
        void set_image_brightness(float value);
        void restore_image_brightness();
        void set_image_contrast(float value);
        void restore_image_contrast();
        void set_saturation( float value);
        void restore_saturation();
        void set_gamma( float value);
        void restore_gamma();
        void restore_image_params();
        uint64_t get_self_render_target() {
            return this->self_render_target_id_;
        }
        uint64_t get_current_render_target() {
            return this->render_target_id_;
        }
    public:
        virtual bool start(const std::string& url);
        virtual void in_context(std::function<void()> task) = 0;
        virtual void set_audio_enabled(bool enabled);
        virtual bool is_audio_enabled();
        virtual bool re_start();
        virtual void stop() = 0;
        virtual bool is_playing() const = 0;
        virtual bool is_live_stream() const = 0;
        // virtual const int64_t &id() const { return id_; }
        // virtual const std::string &url() const { return url_; }

    // 动态调整输出尺寸
        virtual void set_output_size(int width, int height, ScaleQuality quality = ScaleQuality::BILINEAR);

        virtual int get_output_width() const { return target_width_; }
        virtual int get_output_height() const { return target_height_; }
        virtual void update_player_data(const PlayerData& data);
        //for real render to native window
        virtual void render_to_target(uint64_t target_id);
        // 设置回调
        void set_resize_callback(ResizeCallback cb) { resize_callback_ = std::move(cb); }
        //for offscreen render
        void set_frame_sender(RGBFrameSender sender) { frame_sender_ = std::move(sender); };
        void set_message_sender(MessageSender sender) { message_sender_ = std::move(sender); }
        void set_play_status_callback(PlayStatusCallback cb) { play_status_callback_ = std::move(cb); }
        virtual const Stats& get_stats()  { return stats_; }
        void set_play_state(PlayerState state);
        const uint64_t& id()  const { return id_; }
        const std::string& url()  const { return url_; }
        void set_scale_quality(ScaleQuality quality);
        static uint64_t next_render_serial();
    protected:
        virtual void on_video_frame(AVFrame* frame,
            bool first_frame);
        virtual void on_audio_frame(AVFrame* frame);
        void on_error(const std::string& err);
        void notify_play_code(PlayCode code);
        //void set_play_state(PlayerState state);
    protected:
        uint64_t id_{ 0 };
        std::string url_{ "" };
        // 目标尺寸和缩放质量（原子操作保证线程安全）
        std::atomic<int> target_width_{ 1280 };
        std::atomic<int> target_height_{ 720 };
        std::atomic<ScaleQuality> scale_quality_{ ScaleQuality::BICUBIC };
        ResizeCallback resize_callback_{ nullptr };
        //FrameSender frame_sender_{nullptr};
        RGBFrameSender frame_sender_{ nullptr };
        MessageSender message_sender_{ nullptr };
       std::shared_ptr<TaskRenderBase> task_render_{ nullptr };
        PlayStatusCallback play_status_callback_{ nullptr };
        Stats stats_;

        //audio
        std::atomic<bool> audio_initialized_ = false; // 音频设备是否已初始化
        bool has_audio_ = false;
        std::atomic<bool> audio_enabled_{ false };      // 用户是否开启了声音
        std::unique_ptr<SDLAudioPlayer> audio_player_;
        std::atomic<PlayerState> state_{ PlayerState::STOPPED };
        std::atomic<int64_t> video_clock_pts_{ AV_NOPTS_VALUE };
        NativeWindowHandle host_window_;
        bool render_changed_{ false };

        static std::atomic<uint64_t> s_next_id;

        RenderParamValue video_gamma_;
        RenderParamValue video_brightness_;
        RenderParamValue video_contrast_;
        RenderParamValue video_saturation_;
        uint64_t render_target_id_{ 0 };
        uint64_t self_render_target_id_{ 0 };
        uint64_t render_serial_{ 0 };
    };
}