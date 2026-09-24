#ifndef TRACKED_MEMORY_H
#define TRACKED_MEMORY_H

#include <atomic>
#include <iostream>
#include <sstream>
#include <map>
#include <mutex>
#include <thread>
#include <chrono>
#include <type_traits>
#include <functional>
#include <algorithm>
#include <exception>
#include <memory>
namespace BridgePlayer {
    // ==================== 追踪配置 ====================
    enum class TrackLevel {
        NONE,
        LIFECYCLE_ONLY,
        REF_COUNT,
        VERBOSE,
        STACK_TRACE
    };

    struct TrackConfig {
        static TrackLevel level;
        static bool track_leaks;
        static bool track_weak_ptr;
        static bool print_thread_id;
        static std::function<void(const std::string&)> logger;

        static void log(const std::string& msg) {
            if (logger) logger(msg);
            else std::cout << msg << std::endl;
        }
    };

    // 默认配置
    inline TrackLevel TrackConfig::level =
#ifdef TRACK_MEMORY_VERBOSE
        TrackLevel::VERBOSE;
#else
        TrackLevel::REF_COUNT;
#endif

    inline bool TrackConfig::track_leaks =
#ifdef TRACK_MEMORY_LEAKS
        true;
#else
        false;
#endif

    inline bool TrackConfig::track_weak_ptr = true;
    inline bool TrackConfig::print_thread_id = true;
    inline std::function<void(const std::string&)> TrackConfig::logger = nullptr;

    // ==================== 全局追踪器 ====================

    struct TrackInfo {
        long object_id;
        std::string type_name;
        void* address;
        std::atomic<long> ref_count{ 0 };
        std::atomic<long> weak_count{ 0 };
        std::chrono::steady_clock::time_point created_at;
        std::string thread_id;
        bool destroyed{ false };

        // 构造函数
        TrackInfo() = default;

        // 移动构造函数
        TrackInfo(TrackInfo&& other) noexcept
            : object_id(other.object_id)
            , type_name(std::move(other.type_name))
            , address(other.address)
            , ref_count(other.ref_count.load())
            , weak_count(other.weak_count.load())
            , created_at(other.created_at)
            , thread_id(std::move(other.thread_id))
            , destroyed(other.destroyed) {
            // 移动后源对象不再使用，无需重置
        }

        // 移动赋值操作符
        TrackInfo& operator=(TrackInfo&& other) noexcept {
            if (this != &other) {
                object_id = other.object_id;
                type_name = std::move(other.type_name);
                address = other.address;
                ref_count = other.ref_count.load();
                weak_count = other.weak_count.load();
                created_at = other.created_at;
                thread_id = std::move(other.thread_id);
                destroyed = other.destroyed;
            }
            return *this;
        }

        // 禁止拷贝
        TrackInfo(const TrackInfo&) = delete;
        TrackInfo& operator=(const TrackInfo&) = delete;
    };

    class GlobalTracker {
    private:
        static std::mutex mutex_;
        static std::map<void*, TrackInfo> objects_;
        static std::atomic<long> next_id_;

        static std::string get_thread_id() {
            std::ostringstream oss;
            oss << std::this_thread::get_id();
            return oss.str();
        }

    public:
        static long register_object(void* ptr, const char* type_name, long init_ref = 1) {
            if (TrackConfig::level < TrackLevel::LIFECYCLE_ONLY) return 0;

            std::lock_guard<std::mutex> lock(mutex_);
            long id = next_id_++;

            TrackInfo info;
            info.object_id = id;
            info.type_name = type_name;
            info.address = ptr;
            info.ref_count = init_ref;
            info.weak_count = 0;
            info.created_at = std::chrono::steady_clock::now();
            info.thread_id = TrackConfig::print_thread_id ? get_thread_id() : "";
            info.destroyed = false;

            // 使用 emplace 直接构造，避免拷贝/移动
            objects_.emplace(ptr, std::move(info));

            std::ostringstream msg;
            msg << "[CREATE] " << type_name << " #" << id << " at " << ptr
                << ", refs=" << init_ref;
            if (TrackConfig::print_thread_id) msg << ", thread=" << info.thread_id;
            TrackConfig::log(msg.str());

            return id;
        }

        static void unregister_object(void* ptr) {
            if (TrackConfig::level < TrackLevel::LIFECYCLE_ONLY) return;

            std::lock_guard<std::mutex> lock(mutex_);
            auto it = objects_.find(ptr);
            if (it != objects_.end()) {
                it->second.destroyed = true;

                std::ostringstream msg;
                msg << "[DESTROY] " << it->second.type_name << " #" << it->second.object_id
                    << " at " << ptr << ", final refs=" << it->second.ref_count.load();
                TrackConfig::log(msg.str());

                if (TrackConfig::track_leaks && it->second.ref_count.load() > 0) {
                    std::ostringstream leak;
                    leak << "[LEAK] Object #" << it->second.object_id
                        << " destroyed with " << it->second.ref_count.load() << " references!";
                    TrackConfig::log(leak.str());
                }

                objects_.erase(it);
            }
        }

        static void ref_change(void* ptr, int delta, const char* operation) {
            if (TrackConfig::level < TrackLevel::REF_COUNT) return;

            std::lock_guard<std::mutex> lock(mutex_);
            auto it = objects_.find(ptr);
            if (it != objects_.end()) {
                long old = it->second.ref_count.load();
                it->second.ref_count += delta;

                std::ostringstream msg;
                msg << "[REF] " << operation << " " << it->second.type_name
                    << " #" << it->second.object_id << ": " << old << " -> " << it->second.ref_count.load();
                TrackConfig::log(msg.str());
            }
        }

        static void weak_change(void* ptr, int delta) {
            if (!TrackConfig::track_weak_ptr) return;
            if (TrackConfig::level < TrackLevel::REF_COUNT) return;

            std::lock_guard<std::mutex> lock(mutex_);
            auto it = objects_.find(ptr);
            if (it != objects_.end()) {
                long old = it->second.weak_count.load();
                it->second.weak_count += delta;

                if (TrackConfig::level >= TrackLevel::VERBOSE) {
                    std::ostringstream msg;
                    msg << "[WEAK] " << it->second.type_name << " #" << it->second.object_id
                        << ": " << old << " -> " << it->second.weak_count.load();
                    TrackConfig::log(msg.str());
                }
            }
        }

        static void print_status(void* ptr) {
            std::lock_guard<std::mutex> lock(mutex_);
            auto it = objects_.find(ptr);
            if (it != objects_.end()) {
                std::ostringstream msg;
                msg << "[STATUS] " << it->second.type_name << " #" << it->second.object_id
                    << ": refs=" << it->second.ref_count.load()
                    << ", weaks=" << it->second.weak_count.load();
                TrackConfig::log(msg.str());
            }
        }

        static void print_all_leaks() {
            if (!TrackConfig::track_leaks) return;

            std::lock_guard<std::mutex> lock(mutex_);
            bool has_leak = false;
            for (auto& [ptr, info] : objects_) {
                if (!info.destroyed) {
                    has_leak = true;
                    std::ostringstream msg;
                    msg << "[LEAK] " << info.type_name << " #" << info.object_id
                        << " at " << ptr << " still alive with " << info.ref_count.load() << " refs";
                    TrackConfig::log(msg.str());
                }
            }
            if (!has_leak) {
                TrackConfig::log("[OK] No memory leaks detected!");
            }
        }

        static void print_all_objects() {
            std::lock_guard<std::mutex> lock(mutex_);
            TrackConfig::log("=== All Tracked Objects ===");
            for (auto& [ptr, info] : objects_) {
                std::ostringstream msg;
                msg << "  " << info.type_name << " #" << info.object_id
                    << " at " << ptr << ": refs=" << info.ref_count.load()
                    << ", weaks=" << info.weak_count.load();
                TrackConfig::log(msg.str());
            }
        }
    };

    inline std::mutex GlobalTracker::mutex_;
    inline std::map<void*, TrackInfo> GlobalTracker::objects_;
    inline std::atomic<long> GlobalTracker::next_id_{ 1 };

    // ==================== 前向声明 ====================

    template<typename T> class tracked_shared_ptr;
    template<typename T> class tracked_weak_ptr;
    template<typename T> class tracked_enable_shared_from_this;

    // ==================== tracked_weak_ptr ====================

    template<typename T>
    class tracked_weak_ptr {
    private:
        T* ptr_;
        std::atomic<long>* ref_count_;
        std::atomic<long>* weak_count_;

        void release() {
            if (ptr_ && weak_count_) {
                long old = weak_count_->fetch_sub(1);
                GlobalTracker::weak_change(ptr_, -1);

                if (TrackConfig::level >= TrackLevel::VERBOSE) {
                    std::ostringstream msg;
                    msg << "[WEAK_RELEASE] " << ptr_ << ", remaining weaks=" << (old - 1);
                    TrackConfig::log(msg.str());
                }

                if (old == 1 && ref_count_->load() == 0) {
                    if (TrackConfig::level >= TrackLevel::LIFECYCLE_ONLY) {
                        std::ostringstream msg;
                        msg << "[CONTROL_BLOCK] destroyed for " << ptr_;
                        TrackConfig::log(msg.str());
                    }
                    delete ref_count_;
                    delete weak_count_;
                }
            }
            ptr_ = nullptr;
            ref_count_ = nullptr;
            weak_count_ = nullptr;
        }

    public:
        tracked_weak_ptr() : ptr_(nullptr), ref_count_(nullptr), weak_count_(nullptr) {}

        tracked_weak_ptr(const tracked_shared_ptr<T>& shared);

        tracked_weak_ptr(const tracked_weak_ptr& other)
            : ptr_(other.ptr_), ref_count_(other.ref_count_), weak_count_(other.weak_count_) {
            if (ptr_) {
                weak_count_->fetch_add(1);
                GlobalTracker::weak_change(ptr_, 1);

                if (TrackConfig::level >= TrackLevel::VERBOSE) {
                    std::ostringstream msg;
                    msg << "[WEAK_COPY] " << ptr_;
                    TrackConfig::log(msg.str());
                }
            }
        }

        tracked_weak_ptr(tracked_weak_ptr&& other) noexcept
            : ptr_(other.ptr_), ref_count_(other.ref_count_), weak_count_(other.weak_count_) {
            other.ptr_ = nullptr;
            other.ref_count_ = nullptr;
            other.weak_count_ = nullptr;

            if (TrackConfig::level >= TrackLevel::VERBOSE && ptr_) {
                std::ostringstream msg;
                msg << "[WEAK_MOVE] " << ptr_;
                TrackConfig::log(msg.str());
            }
        }

        ~tracked_weak_ptr() {
            release();
        }

        tracked_weak_ptr& operator=(const tracked_weak_ptr& other) {
            if (this != &other) {
                release();
                ptr_ = other.ptr_;
                ref_count_ = other.ref_count_;
                weak_count_ = other.weak_count_;
                if (ptr_) {
                    weak_count_->fetch_add(1);
                    GlobalTracker::weak_change(ptr_, 1);
                }
            }
            return *this;
        }

        tracked_weak_ptr& operator=(const tracked_shared_ptr<T>& shared);

        tracked_shared_ptr<T> lock() const;

        bool expired() const {
            return !ptr_ || !ref_count_ || ref_count_->load() == 0;
        }

        long use_count() const {
            return ref_count_ ? ref_count_->load() : 0;
        }

        void reset() {
            if (TrackConfig::level >= TrackLevel::VERBOSE && ptr_) {
                std::ostringstream msg;
                msg << "[WEAK_RESET] " << ptr_;
                TrackConfig::log(msg.str());
            }
            release();
        }

        void print_status() const {
            std::ostringstream msg;
            msg << "[WEAK_PTR] ptr=" << ptr_
                << ", expired=" << (expired() ? "yes" : "no")
                << ", refs=" << use_count()
                << ", weaks=" << (weak_count_ ? weak_count_->load() : 0);
            TrackConfig::log(msg.str());
        }

        T* get() const { return ptr_; }

    private:
        template<typename U>
        friend class tracked_shared_ptr;
    };

    // ==================== tracked_shared_ptr ====================

    template<typename T>
    class tracked_shared_ptr {
    private:
        T* ptr_;
        std::atomic<long>* ref_count_;
        std::atomic<long>* weak_count_;

        void release() {
            if (ptr_ && ref_count_) {
                long old = ref_count_->fetch_sub(1);
                long new_val = old - 1;

                // 总是记录减少
                GlobalTracker::ref_change(ptr_, -1, "release");

                if (TrackConfig::level >= TrackLevel::VERBOSE) {
                    std::ostringstream msg;
                    msg << "[RELEASE] " << typeid(T).name() << " at " << ptr_
                        << ", refs: " << old << " -> " << new_val;
                    TrackConfig::log(msg.str());
                }

                // 只有最后一个 shared_ptr 才删除对象
                if (old == 1) {
                    // 最后一个 shared_ptr，删除对象
                    if (weak_count_->load() == 0) {
                        if (TrackConfig::level >= TrackLevel::LIFECYCLE_ONLY) {
                            std::ostringstream msg;
                            msg << "[CONTROL_BLOCK] destroyed for " << ptr_;
                            TrackConfig::log(msg.str());
                        }
                        delete ref_count_;
                        delete weak_count_;
                    }

                    if (TrackConfig::level >= TrackLevel::LIFECYCLE_ONLY) {
                        std::ostringstream msg;
                        msg << "[DELETE] Object at " << ptr_;
                        TrackConfig::log(msg.str());
                    }

                    delete ptr_;
                    GlobalTracker::unregister_object(ptr_);
                    ptr_ = nullptr;
                    ref_count_ = nullptr;
                    weak_count_ = nullptr;
                }
            }
        }

        void setup_weak_this() {
            if (ptr_) {
                if constexpr (std::is_base_of_v<tracked_enable_shared_from_this<T>, T>) {
                    ptr_->set_weak_this(*this);
                }
            }
        }

    public:
        explicit tracked_shared_ptr(T* ptr = nullptr) : ptr_(ptr) {
            if (ptr_) {
                ref_count_ = new std::atomic<long>(1);
                weak_count_ = new std::atomic<long>(0);

                GlobalTracker::register_object(ptr_, typeid(T).name(), 1);
                setup_weak_this();

                if (TrackConfig::level >= TrackLevel::VERBOSE) {
                    std::ostringstream msg;
                    msg << "[SHARED_CTOR] ptr=" << ptr_ << ", refs=1";
                    TrackConfig::log(msg.str());
                }
            }
            else {
                ref_count_ = nullptr;
                weak_count_ = nullptr;
            }
        }

        explicit tracked_shared_ptr(const tracked_weak_ptr<T>& weak)
            : ptr_(weak.ptr_), ref_count_(weak.ref_count_), weak_count_(weak.weak_count_) {
            if (ptr_) {
                long old = ref_count_->fetch_add(1);
                if (old == 0) {
                    ref_count_->fetch_sub(1);
                    ptr_ = nullptr;
                    ref_count_ = nullptr;
                    weak_count_ = nullptr;
                    throw std::bad_weak_ptr();
                }
                GlobalTracker::ref_change(ptr_, 1, "from_weak_ptr");

                if (TrackConfig::level >= TrackLevel::VERBOSE) {
                    std::ostringstream msg;
                    msg << "[SHARED_FROM_WEAK] " << ptr_ << ", refs=" << use_count();
                    TrackConfig::log(msg.str());
                }
            }
        }

        tracked_shared_ptr(const tracked_shared_ptr& other)
            : ptr_(other.ptr_), ref_count_(other.ref_count_), weak_count_(other.weak_count_) {
            if (ptr_) {
                ref_count_->fetch_add(1);
                GlobalTracker::ref_change(ptr_, 1, "copy_ctor");
            }

            if (TrackConfig::level >= TrackLevel::VERBOSE) {
                std::ostringstream msg;
                msg << "[SHARED_COPY] ptr=" << ptr_ << ", refs=" << use_count();
                TrackConfig::log(msg.str());
            }
        }

        tracked_shared_ptr(tracked_shared_ptr&& other) noexcept
            : ptr_(other.ptr_), ref_count_(other.ref_count_), weak_count_(other.weak_count_) {
            other.ptr_ = nullptr;
            other.ref_count_ = nullptr;
            other.weak_count_ = nullptr;

            if (TrackConfig::level >= TrackLevel::VERBOSE && ptr_) {
                std::ostringstream msg;
                msg << "[SHARED_MOVE] ptr=" << ptr_ << ", refs=" << use_count();
                TrackConfig::log(msg.str());
            }
        }

        ~tracked_shared_ptr() {
            if (TrackConfig::level >= TrackLevel::VERBOSE && ptr_) {
                std::ostringstream msg;
                msg << "[SHARED_DTOR] " << typeid(T).name()
                    << " at " << ptr_
                    << ", refs before=" << use_count()
                    << ", will become=" << (use_count() - 1);
                TrackConfig::log(msg.str());
            }
            release();
        }

        tracked_shared_ptr& operator=(const tracked_shared_ptr& other) {
            if (this != &other) {
                release();
                ptr_ = other.ptr_;
                ref_count_ = other.ref_count_;
                weak_count_ = other.weak_count_;
                if (ptr_) {
                    ref_count_->fetch_add(1);
                    GlobalTracker::ref_change(ptr_, 1, "assignment");
                }
            }
            return *this;
        }

        tracked_shared_ptr& operator=(tracked_shared_ptr&& other) noexcept {
            if (this != &other) {
                release();
                ptr_ = other.ptr_;
                ref_count_ = other.ref_count_;
                weak_count_ = other.weak_count_;
                other.ptr_ = nullptr;
                other.ref_count_ = nullptr;
                other.weak_count_ = nullptr;
            }
            return *this;
        }

        tracked_shared_ptr& operator=(T* ptr) {
            if (ptr_ != ptr) {
                if (TrackConfig::level >= TrackLevel::VERBOSE) {
                    std::ostringstream msg;
                    msg << "[SHARED_ASSIGN_RAW] from " << ptr_ << " to " << ptr;
                    TrackConfig::log(msg.str());
                }

                release();
                ptr_ = ptr;
                if (ptr_) {
                    ref_count_ = new std::atomic<long>(1);
                    weak_count_ = new std::atomic<long>(0);
                    GlobalTracker::register_object(ptr_, typeid(T).name(), 1);
                    setup_weak_this();
                }
                else {
                    ref_count_ = nullptr;
                    weak_count_ = nullptr;
                }
            }
            return *this;
        }

        T* operator->() const { return ptr_; }
        T& operator*() const { return *ptr_; }
        T* get() const { return ptr_; }

        long use_count() const {
            return ref_count_ ? ref_count_->load() : 0;
        }

        bool unique() const {
            return use_count() == 1;
        }

        explicit operator bool() const {
            return ptr_ != nullptr;
        }

        void reset(T* ptr = nullptr) {
            if (TrackConfig::level >= TrackLevel::VERBOSE) {
                std::ostringstream msg;
                msg << "[SHARED_RESET] from " << ptr_ << " to " << ptr;
                TrackConfig::log(msg.str());
            }

            release();

            ptr_ = ptr;
            if (ptr_) {
                ref_count_ = new std::atomic<long>(1);
                weak_count_ = new std::atomic<long>(0);
                GlobalTracker::register_object(ptr_, typeid(T).name(), 1);
                setup_weak_this();
            }
            else {
                ref_count_ = nullptr;
                weak_count_ = nullptr;
            }
        }

        void swap(tracked_shared_ptr& other) noexcept {
            std::swap(ptr_, other.ptr_);
            std::swap(ref_count_, other.ref_count_);
            std::swap(weak_count_, other.weak_count_);
        }

        void print_status() const {
            std::ostringstream msg;
            msg << "[SHARED_PTR] ptr=" << ptr_ << ", refs=" << use_count();
            TrackConfig::log(msg.str());
        }

    private:
        template<typename U>
        friend class tracked_weak_ptr;

        template<typename U>
        friend class tracked_enable_shared_from_this;
    };

    // ==================== 实现依赖的方法 ====================

    template<typename T>
    tracked_weak_ptr<T>::tracked_weak_ptr(const tracked_shared_ptr<T>& shared)
        : ptr_(shared.ptr_), ref_count_(shared.ref_count_), weak_count_(shared.weak_count_) {
        if (ptr_) {
            weak_count_->fetch_add(1);
            GlobalTracker::weak_change(ptr_, 1);

            if (TrackConfig::level >= TrackLevel::VERBOSE) {
                std::ostringstream msg;
                msg << "[WEAK_CTOR] from shared_ptr to " << ptr_;
                TrackConfig::log(msg.str());
            }
        }
    }

    template<typename T>
    tracked_weak_ptr<T>& tracked_weak_ptr<T>::operator=(const tracked_shared_ptr<T>& shared) {
        release();
        ptr_ = shared.ptr_;
        ref_count_ = shared.ref_count_;
        weak_count_ = shared.weak_count_;
        if (ptr_) {
            weak_count_->fetch_add(1);
            GlobalTracker::weak_change(ptr_, 1);
        }
        return *this;
    }

    template<typename T>
    tracked_shared_ptr<T> tracked_weak_ptr<T>::lock() const {
        if (expired()) {
            if (TrackConfig::level >= TrackLevel::VERBOSE) {
                std::ostringstream msg;
                msg << "[WEAK_LOCK] failed, object expired at " << ptr_;
                TrackConfig::log(msg.str());
            }
            return tracked_shared_ptr<T>();
        }

        if (TrackConfig::level >= TrackLevel::VERBOSE) {
            std::ostringstream msg;
            msg << "[WEAK_LOCK] success, refs=" << use_count();
            TrackConfig::log(msg.str());
        }

        return tracked_shared_ptr<T>(*this);
    }

    // ==================== make_tracked_shared ====================

    template<typename T, typename... Args>
    tracked_shared_ptr<T> make_tracked_shared(Args&&... args) {
        return tracked_shared_ptr<T>(new T(std::forward<Args>(args)...));
    }

    // ==================== tracked_enable_shared_from_this ====================

    template<typename T>
    class tracked_enable_shared_from_this {
    private:
        mutable tracked_weak_ptr<T> tracked_weak_this_;
        mutable long object_id_{ 0 };

    protected:
        tracked_enable_shared_from_this() {
            if (TrackConfig::level >= TrackLevel::LIFECYCLE_ONLY) {
                std::ostringstream msg;
                msg << "[CTOR] tracked_enable_shared_from_this for " << typeid(T).name()
                    << " at " << static_cast<T*>(this);
                TrackConfig::log(msg.str());
            }
        }

        tracked_enable_shared_from_this(const tracked_enable_shared_from_this&) {
            if (TrackConfig::level >= TrackLevel::VERBOSE) {
                std::ostringstream msg;
                msg << "[COPY] tracked_enable_shared_from_this for " << typeid(T).name();
                TrackConfig::log(msg.str());
            }
        }

        tracked_enable_shared_from_this& operator=(const tracked_enable_shared_from_this&) {
            return *this;
        }

        ~tracked_enable_shared_from_this() {
            if (TrackConfig::level >= TrackLevel::LIFECYCLE_ONLY && object_id_) {
                std::ostringstream msg;
                msg << "[DTOR] tracked_enable_shared_from_this for #" << object_id_;
                TrackConfig::log(msg.str());
            }
        }

    public:
        tracked_shared_ptr<T> shared_from_this() {
            auto ptr = tracked_weak_this_.lock();
            if (!ptr) {
                std::ostringstream msg;
                msg << "[ERROR] shared_from_this() failed for object";
                if (object_id_) msg << " #" << object_id_;
                msg << " at " << static_cast<T*>(this);
                TrackConfig::log(msg.str());
                throw std::bad_weak_ptr();
            }

            if (TrackConfig::level >= TrackLevel::VERBOSE) {
                std::ostringstream msg;
                msg << "[SHARED_FROM_THIS] #" << object_id_
                    << ", refs=" << ptr.use_count();
                TrackConfig::log(msg.str());
            }

            return ptr;
        }

        tracked_shared_ptr<const T> shared_from_this() const {
            return const_cast<tracked_enable_shared_from_this*>(this)->shared_from_this();
        }

        tracked_weak_ptr<T> weak_from_this() {
            if (TrackConfig::level >= TrackLevel::VERBOSE) {
                std::ostringstream msg;
                msg << "[WEAK_FROM_THIS] #" << object_id_;
                TrackConfig::log(msg.str());
            }
            return tracked_weak_this_;
        }

        tracked_weak_ptr<const T> weak_from_this() const {
            return const_cast<tracked_enable_shared_from_this*>(this)->weak_from_this();
        }

        // 调试接口
        bool has_shared_ptr() const {
            return !tracked_weak_this_.expired();
        }

        long object_id() const { return object_id_; }
        long use_count() const { return tracked_weak_this_.use_count(); }
        bool expired() const { return tracked_weak_this_.expired(); }

        void print_status() const {
            tracked_weak_this_.print_status();
        }

    private:
        template<typename U>
        friend class tracked_shared_ptr;

        void set_weak_this(const tracked_shared_ptr<T>& ptr) const {
            if (tracked_weak_this_.expired()) {
                tracked_weak_this_ = tracked_weak_ptr<T>(ptr);
                object_id_ = GlobalTracker::register_object(
                    const_cast<T*>(static_cast<const T*>(this)),
                    typeid(T).name(),
                    ptr.use_count()
                );

                if (TrackConfig::level >= TrackLevel::VERBOSE) {
                    std::ostringstream msg;
                    msg << "[SET_WEAK_THIS] #" << object_id_ << " for object at " << this;
                    TrackConfig::log(msg.str());
                }
            }
            else if (TrackConfig::level >= TrackLevel::VERBOSE) {
                std::ostringstream msg;
                msg << "[SET_WEAK_THIS] Already set for object at " << this;
                TrackConfig::log(msg.str());
            }
        }
    };
}
#endif // TRACKED_MEMORY_H