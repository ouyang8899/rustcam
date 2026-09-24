// task_executor.h
#pragma once

#include <functional>
#include <future>
#include <thread>
#include <vector>
#include <queue>
#include <mutex>
#include <condition_variable>
#include <type_traits>
#include <stdexcept>
#include "core/bridge_export.h"

namespace BridgePlayer
{
    class BRIDGE_EXPORT TaskExecutor
    {
    public:
        explicit TaskExecutor(size_t num_threads);
        ~TaskExecutor();

        // 禁止拷贝和移动
        TaskExecutor(const TaskExecutor&) = delete;
        TaskExecutor& operator=(const TaskExecutor&) = delete;
        TaskExecutor(TaskExecutor&&) = delete;
        TaskExecutor& operator=(TaskExecutor&&) = delete;

        // 提交任务并返回 future（C++14 兼容版本）
        template <typename F, typename... Args>
        auto submit(F&& f, Args &&...args)
            -> std::future<typename std::result_of<F(Args...)>::type>
        {
            using return_type = typename std::result_of<F(Args...)>::type;

            auto task = std::make_shared<std::packaged_task<return_type()>>(
                std::bind(std::forward<F>(f), std::forward<Args>(args)...));

            std::future<return_type> result = task->get_future();
            {
                std::unique_lock<std::mutex> lock(mutex_);
                if (stop_)
                {
                    throw std::runtime_error("TaskExecutor is shutting down, cannot accept new tasks");
                }
                tasks_.emplace([task]() { (*task)(); });
            }
            cv_.notify_one();
            return result;
        }

        // 提交任务，不关心返回值
        template <typename F, typename... Args>
        void submit_detached(F&& f, Args &&...args)
        {
            auto task = std::make_shared<std::packaged_task<void()>>(
                std::bind(std::forward<F>(f), std::forward<Args>(args)...));
            {
                std::unique_lock<std::mutex> lock(mutex_);
                if (stop_)
                {
                    throw std::runtime_error("TaskExecutor is shutting down, cannot accept new tasks");
                }
                tasks_.emplace([task]() {
                    try {
                        (*task)();
                    }
                    catch (...) {
                        // 分离任务：吞掉异常，避免 std::terminate
                        // 生产环境建议添加日志记录
                    }
                    });
            }
            cv_.notify_one();
        }

        // 查询状态
        size_t pending_tasks() const;
        size_t thread_count() const;
        bool is_running() const;

        // 优雅关闭：等待所有已提交任务完成后退出
        void shutdown();

    private:
        void worker_loop();

        std::vector<std::thread> workers_;
        std::queue<std::function<void()>> tasks_;
        mutable std::mutex mutex_;
        std::condition_variable cv_;
        bool stop_ = false;
        size_t active_threads_ = 0;
    };
}