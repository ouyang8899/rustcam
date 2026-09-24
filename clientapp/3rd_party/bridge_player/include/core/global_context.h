#pragma once
#include"core/bridge_export.h"
#include<string>
#include"manager/player_types.h"
#include<vector>
#include"config/bridge_config.h"
namespace BridgePlayer{
    BRIDGE_EXPORT  void  test_render();
    class BRIDGE_EXPORT GlobalContext {
    public:
        static GlobalContext& instance();
    public:
        void install(const std::vector<RenderTargetData>& render_target_vec);
        void add_render_target(const std::vector<RenderTargetData>& render_target_vec, int max_queue_size);
        void uninstall();
        void set_rtsp_transport_type(const std::string& transport);
        PlayerConfig::Ptr get_player_config();
        void set_player_config(PlayerConfig::Ptr config);
        const std::string& get_rtsp_transport_type();
    protected:
        std::string rtsp_transport_{ "UDP" };
        PlayerConfig::Ptr player_config_;
    };
}