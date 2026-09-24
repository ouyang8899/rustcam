#pragma once
#include <thread>
#include <memory>
#include <functional>
#include<list>
#include <mutex>
#include"core/bridge_export.h"
namespace BridgePlayer
{
    using RunTask = std::function<void()>;
    class BRIDGE_EXPORT AppRunner
    {
    public:
        AppRunner();
        void post_task(RunTask task);

    protected:
        void process_tasks();
        bool has_task();

    private:
        std::list<RunTask> task_list_;
        std::mutex task_list_mutex_;
    };
}
