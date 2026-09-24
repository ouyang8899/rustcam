#pragma once
namespace BridgePlayer {
    enum class PlayCode {
        CONNECTING = 0,
        RE_CONNECT,
        CANT_CONNECT,
        CONNECTED,
        START_PLAY,
        FIRST_FRAME,
        DISCONNECT,
        LOSE_SESSION,
        STOP_PLAY,
        MEDIA_ERROR,

    };
}
