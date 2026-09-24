#pragma once
#include <cstdio>
#include <cstdarg>
#include <atomic>
#include <condition_variable>
#include <ctime>
#include <fstream>
#include <iostream>
#include <mutex>
#include <queue>
#include <sstream>
#include <string>
#include <thread>
#include "core/bridge_export.h"
#ifdef _WIN32
#include <windows.h>
#else
#include <sys/syscall.h>
#include <unistd.h>
#endif

// ================================================================
//                           日志等级
// ================================================================
namespace BridgePlayer
{
    enum class LogLevel
    {
        TRACE = 0,
        DEBUG = 1,
        INFO = 2,
        WARN = 3,
        ERROR_ = 4,
        FATAL = 5,
        OFF = 6
    };

    inline const char *log_level_name(LogLevel level)
    {
        switch (level)
        {
        case LogLevel::TRACE:
            return "TRACE";
        case LogLevel::DEBUG:
            return "DEBUG";
        case LogLevel::INFO:
            return "INFO ";
        case LogLevel::WARN:
            return "WARN ";
        case LogLevel::ERROR_:
            return "ERROR";
        case LogLevel::FATAL:
            return "FATAL";
        default:
            return "UNKNOW";
        }
    }

    // ================================================================
    //                           Logger 类
    // ================================================================
    class BRIDGE_EXPORT Logger
    {
    public:
        enum class Mode
        {
            SYNC,
            ASYNC
        };

        static Logger &instance();

        bool init(const std::string &log_dir,
                  LogLevel min_level = LogLevel::INFO,
                  Mode mode = Mode::ASYNC,
                  bool console_output = true);

        void shutdown();

        void set_min_level(LogLevel level);
        LogLevel get_min_level() const { return min_level_.load(std::memory_order_acquire); }
        void set_console_output(bool enabled);

        void log(LogLevel level, const char *file, int line, const char *func,
                 const std::string &message);

        Logger(const Logger &) = delete;
        Logger &operator=(const Logger &) = delete;

    private:
        Logger() = default;
        ~Logger();

        void async_loop();
        void open_new_file();
        void write_line(const std::string &line);

        static std::string get_hourly_filename(const std::string &dir);
        static std::string get_timestamp();
        static std::string get_thread_id();
        static std::tm get_local_time(std::time_t t);

        Mode mode_ = Mode::SYNC;
        std::atomic<LogLevel> min_level_{LogLevel::INFO};
        std::atomic<bool> console_output_{true};
        std::atomic<bool> running_{false};

        std::string log_dir_;

        std::ofstream file_stream_;
        int current_hour_ = -1;
        std::chrono::steady_clock::time_point last_hour_check_;

        std::thread async_thread_;
        std::queue<std::string> queue_;
        std::mutex mutex_;
        std::condition_variable cv_;
        static constexpr size_t MAX_QUEUE_SIZE = 10000;
        std::atomic<size_t> dropped_count_{0}; // 丢弃的日志条数
    };

    // ================================================================
    //                          LogStream（流式宏）
    // ================================================================
    class LogStream
    {
    public:
        LogStream(LogLevel level, const char *file, int line, const char *func)
            : level_(level), file_(file), line_(line), func_(func) {}

        ~LogStream()
        {
            Logger::instance().log(level_, file_, line_, func_, stream_.str());
        }

        std::ostream &stream() { return stream_; }

    private:
        LogLevel level_;
        const char *file_;
        int line_;
        const char *func_;
        std::ostringstream stream_;
    };
    class RunnerLog
    {
    public:
        RunnerLog(std::chrono::seconds seconds = std::chrono::seconds(10))
        {
            time_span_ = seconds;
            timeout_ = std::chrono::steady_clock::now() + time_span_;
        }

    public:
        void print(const std::string &msg)
        {
            if (std::chrono::steady_clock::now() < timeout_)
            {
                return;
            }
            timeout_ = timeout_ + time_span_;
            BridgePlayer::Logger::instance().log(BridgePlayer::LogLevel::DEBUG, __FILE__, __LINE__, __func__, msg);
        }

    protected:
        std::chrono::steady_clock::time_point timeout_;
        std::chrono::seconds time_span_;
    };
}

// ================================================================
//                          便捷宏
// ================================================================
#define APP_LOG_TRACE(msg) \
    BridgePlayer::Logger::instance().log(BridgePlayer::LogLevel::TRACE, __FILE__, __LINE__, __func__, msg)

#define APP_LOG_DEBUG(msg) \
    BridgePlayer::Logger::instance().log(BridgePlayer::LogLevel::DEBUG, __FILE__, __LINE__, __func__, msg)

#define APP_LOG_INFO(msg) \
    BridgePlayer::Logger::instance().log(BridgePlayer::LogLevel::INFO, __FILE__, __LINE__, __func__, msg)

#define APP_LOG_WARN(msg) \
    BridgePlayer::Logger::instance().log(BridgePlayer::LogLevel::WARN, __FILE__, __LINE__, __func__, msg)

#define APP_LOG_ERROR(msg) \
    BridgePlayer::Logger::instance().log(BridgePlayer::LogLevel::ERROR_, __FILE__, __LINE__, __func__, msg)

#define APP_LOG_FATAL(msg) \
    BridgePlayer::Logger::instance().log(BridgePlayer::LogLevel::FATAL, __FILE__, __LINE__, __func__, msg)

#define APP_LOG_STREAM(level)                                    \
    if (level >= BridgePlayer::Logger::instance().get_min_level()) \
    BridgePlayer::LogStream(level, __FILE__, __LINE__, __func__).stream()

#ifdef _WIN32
static void debug_log(const char *fmt, ...)
{
#if __TEST__
    return;
    // char buf[512];
    // va_list args;
    // va_start(args, fmt);
    // vsnprintf(buf, sizeof(buf), fmt, args);
    // va_end(args);
    // APP_LOG_DEBUG(buf);
#endif
}

#else
static void debug_log(const char *fmt, ...)
{
}
#endif
