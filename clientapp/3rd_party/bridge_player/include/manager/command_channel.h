#pragma once
#include"core/bridge_export.h"
#include<memory>
#include"manager/player_base.h"
#include<unordered_map>
#include<mutex>
#include<vector>
#include<json/json.h>
#include<thread>
#include<queue>
#include<chrono>
namespace BridgePlayer{
    class BRIDGE_EXPORT CommandChannel {
    public:
        using Ptr = std::shared_ptr<CommandChannel>;
        class SendPlayerData {
        public:
            using Ptr = std::shared_ptr<SendPlayerData>;
        public:
            int64_t player_id;
            int64_t render_target_id;
            std::vector<uint8_t> data;
            int width;
            int height;
            int64_t pts;
            int color_type;
            bool use_compreed;
            void to_binary(std::vector<uint8_t>& data);
        };
    public:
        CommandChannel();
        virtual ~CommandChannel();
    public:
        const std::unordered_map<uint64_t, WeakPtr<PlayerBase>>& get_attached_players();
        void attach_player(WeakPtr<PlayerBase> player);
        void detach_player(uint64_t id);
        virtual void send_json(const Json::Value& data) = 0;
        virtual void send_binary(const std::vector<uint8_t>& data) = 0;
    protected:
        void send_player_frame_data(
            const int64_t& player_id,
            const int64_t& render_target_id,
            std::vector<uint8_t>& data,
            int width, int height,
            int64_t pts,
            int color_type,
            bool use_compreed);
        void send_work_imp();
        SendPlayerData::Ptr get_one_player_data();
    protected:
        std::mutex player_map_mutex_;
        std::unordered_map<uint64_t, WeakPtr<PlayerBase>> player_map_;
        std::unique_ptr<std::thread> send_thread_{ nullptr };
        std::vector<SendPlayerData::Ptr> send_data_queue_;
        std::mutex send_data_queue_mutex_;
        std::condition_variable send_data_queue_cv_;
       // std::queue<SendPlayerData::Ptr> send_data_queue_;
        bool running_{ false };

        friend class ClientCommandHandler;
    };
}