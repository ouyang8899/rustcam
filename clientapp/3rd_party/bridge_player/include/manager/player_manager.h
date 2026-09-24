#pragma once
#include <boost/asio.hpp>
#include <memory>
#include <unordered_map>
#include <shared_mutex>
#include <string>
#include <vector>
#include"manager/player_base.h"
#include"core/bridge_export.h"
#include"manager/command_handler.h"
#include"platform/native_window_handle.h"
#include"manager/player_types.h"
//#include"websocket/frame_publisher.h"
//class CommandHandler;
namespace asio = boost::asio;
//using  ConnectionInfoPtr = std::shared_ptr<FramePublisher::ConnectionInfo>;
namespace BridgePlayer {
    class  BRIDGE_EXPORT PlayerManager {
    public:
        explicit PlayerManager();
        ~PlayerManager();
    public:
        static PlayerManager& instance() {
            static PlayerManager sInstance;
            return sInstance;
        }
     /*   bool add_player(const uint64_t& id, const std::string& url);*/
        uint64_t add_player(const uint64_t& render_target_id, const uint64_t& id = 0,NativeWindowHandle handle=nullptr);
        uint64_t add_media_player(
            const uint64_t& render_target_id,
            const uint64_t& id = 0,
            NativeWindowHandle handle = nullptr);
        bool play(const uint64_t& id, const std::string& url);
        void stop(const uint64_t& id);
        PlayerBase::Ptr get_player(const uint64_t id,bool remove=false);
        void remove_player(const uint64_t& id);
    /*    void add_players(const std::vector<std::pair<uint64_t, std::string>>& streams);*/
        void start_all();
        void stop_all();
        void sync_players(const std::vector<PlayerData>& playerDatas);
        struct Stats { int total = 0, playing = 0; };
        Stats get_stats() const;
        void print_stats();

        void set_player_output_size(const uint64_t& id, int width, int height, ScaleQuality quality = ScaleQuality::BILINEAR);
        void set_all_output_size(int width, int height, ScaleQuality quality = ScaleQuality::BILINEAR);
        const std::unordered_map<uint64_t, PlayerBase::Ptr>& get_active_players();
        void close_channel_players(CommandChannel::Ptr channel);
        void handle_command(CommandChannel::Ptr channel,
           const std::string& type, const std::string& requestId, const std::string& cRequestId,const
           Json::Value& data);
        void set_command_handler(const std::shared_ptr<CommandHandler>& handler);
        bool build_render_target(const RenderTargetData& rtd);
    private:
        std::unordered_map<uint64_t, PlayerBase::Ptr> players_;
        mutable std::shared_mutex mutex_;
        std::shared_ptr<CommandHandler> command_handler_;
    };
}
