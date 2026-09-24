#pragma once
#include <cstddef>
#include<boost/utility/string_view.hpp>
#if defined(_MSC_VER)
#   define CPP_STD_VER _MSVC_LANG
#else
#   define CPP_STD_VER __cplusplus
#endif
namespace BridgePlayer {
#if CPP_STD_VER >= 201703L
#   include <string_view>
	using bp_string_view = std::string_view;
#else
	// 已经include boost string_view头文件
	using bp_string_view = boost::string_view;
#endif
}
