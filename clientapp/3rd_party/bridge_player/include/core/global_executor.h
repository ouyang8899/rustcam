// global_executor.h
#pragma once
#include"core/bridge_export.h"
#include "task_executor.h"
namespace BridgePlayer {
	// 返回全局唯一实例
	BRIDGE_EXPORT TaskExecutor& global_executor();

	void init_global_executor(size_t num_threads);
}
