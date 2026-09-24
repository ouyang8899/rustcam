#pragma once
#include"core/bridge_export.h"
namespace BridgePlayer {
	class BRIDGE_EXPORT PlatformUtil {

	public:
#ifdef _WIN32
		static bool Is_windows10_or_later();
#endif
	};
}
