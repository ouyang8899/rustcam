#pragma once
#include <cstdint>
#include <string>
#include<memory>
namespace BridgePlayer {
    struct PlayerConfig {
        using Ptr = std::shared_ptr<PlayerConfig>;
        // UDP 端口范围
        int udp_port_min = 30000;
        int udp_port_max = 60000;
    };
}
