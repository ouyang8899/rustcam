#pragma once
#include <boost/asio.hpp>
#include <memory>
#include <string>
#include <atomic>
#include <functional>
#include <map>
#include<unordered_map>
#include <mutex>
#include "core/app_runner.h"
#include"core/bridge_export.h"
extern "C"
{
#include <libavutil/frame.h>
#include <libavcodec/codec_id.h>
#include "core/player_code.h"
}
#include "manager/player_base.h"

namespace asio = boost::asio;
//struct uv_loop_t;
namespace BridgePlayer {
    class PlayerManager;
}

namespace BridgePlayer
{
    class RTPDepacketizer;
    class AudioRTPDepacketizer;
    class RTSPSession;
    class VideoDecoder;
    class YUVConverter;
    class AudioDecoder;
    struct StreamData;
    class PSTSDecoder;
    struct MediaTrack;
    class PSStreamDemuxer;

    class BRIDGE_EXPORT Player : public PlayerBase
    {
    public:
        using Ptr = SharedPtr<Player>;
        using WPtr = WeakPtr<Player>;
    protected:
        explicit Player(const uint64_t &id,
            const uint64_t& render_target_id,
            NativeWindowHandle handle= nullptr);
       Player() = delete;
       Player(const Player& other) = delete;
       Player(Player&& other) = delete;
    public:
        SharedPtr<Player> child_shared_from_this() {
            return std::static_pointer_cast<Player>(PlayerBase::shared_from_this());
        }
         ~Player();

        bool start(const std::string &url) override;
        void in_context(std::function<void()> task) override;
        //void set_audio_enabled(bool enabled) override;
        bool re_start() override;
        void stop() override;
        bool is_playing()  const override {
            return this->state_ == PlayerState::CONNECTING ||
            this->state_ == PlayerState::PLAYING ||
            this->state_ == PlayerState::CONNECTED; }
        bool is_live_stream() const override {
            return true;
        }

        //void render_to_window(NativeWindowHandle host) override;
       // void capture_video_image(const std::string& fullPath) override;


    protected:
      //  void on_decoded_frame(AVFrame *frame, bool first_frame) override;
      //  void on_audio_frame(AVFrame *frame) override;
    protected:
        void do_work();
        void process_stream_data(std::shared_ptr<StreamData> stream_data);
        void post_task(const Task &task);
        void update_depacketizer(int track_id, bool use_tcp);

        void create_codec_context(const std::unordered_map<int,std::shared_ptr<MediaTrack>>& media_tracks);
    private:
        void on_rtp_packet(const uint8_t *data, size_t size, int64_t pts, bool is_key);
        void on_rtp_stream_data_packet(std::shared_ptr<StreamData> stream_data);


        void init_video_decoder(AVCodecID codec_id);

        void on_audio_rtp_packet(const uint8_t *data, size_t size, int64_t pts);

        void init_audio_decoder(AVCodecID codec_id, std::shared_ptr<MediaTrack> media_track);
        void init_ps_ts_decoder(const std::unique_ptr<PSStreamDemuxer> &stream_demuxer);
        // 重新创建转换器
        void recreate_converter();
        void process_tasks();

        std::shared_ptr<StreamData> get_one_stream_data();
        //int64_t render_id_{-1};
        void *event_loop_{nullptr};
        size_t event_loop_index_{0};
        SharedPtr<RTSPSession> rtsp_session_;
        std::unique_ptr<VideoDecoder> decoder_{nullptr};
        //std::unique_ptr<YUVUtil::YUV420PConverter> converter_;
        std::unique_ptr<AudioDecoder> audio_decoder_{nullptr};
        std::unique_ptr<PSTSDecoder> ps_ts_decoder_{nullptr};
        bool running_{false};
        std::shared_ptr<std::thread> work_thread_{nullptr};
        std::list<Task> task_list_;
        std::list<std::shared_ptr<StreamData>> stream_data_list_;
        std::mutex stream_data_list_mutex_;
        std::condition_variable stream_data_list_cv_;
        std::map<int, std::unique_ptr<RTPDepacketizer>> depacketizers_;
        std::unordered_map<int, std::unique_ptr<AudioRTPDepacketizer>> audio_depacketizers_;
        std::mutex worker_thread_mutex_;
        double media_rate_{ 0 };
        public:
        friend class PlayerManager;
        friend class RTSPSession;
    };
}
