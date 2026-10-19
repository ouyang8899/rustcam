#pragma once
#include <cstdint>
#include <string>
#include<memory>
#include"manager/player_types.h"
namespace BridgePlayer {
    typedef struct PlayerConfig {
        using Ptr = std::shared_ptr<PlayerConfig>;
        // UDP 端口范围
        int udp_port_min = 30000;
        int udp_port_max = 60000;
        uint16_t video_decode_type{ (uint16_t)VideoDecodeType::AUTO };
    }PlayerConfig;
}
