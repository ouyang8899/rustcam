#pragma once

#include "manager/media/media_error.h"
#include <boost/variant.hpp>
#include <stdexcept>
#include <utility>
#include <type_traits>

namespace BridgePlayer {

// ============================================================
// 空类型
// ============================================================
struct EmptyResult {};

// ============================================================
// Result访问异常
// ============================================================
class BadResultAccess : public std::runtime_error {
public:
    explicit BadResultAccess(const std::string& msg)
        : std::runtime_error(msg) {}
};

// ============================================================
// Result<T> 主模板
// ============================================================
template<typename T>
class Result {
public:
    using ValueType = T;
    using ErrorType = MediaError;

    // ---- 构造函数 ----

    Result() : storage_(T()) {}

    Result(const T& value) : storage_(value) {}
    Result(T&& value) : storage_(std::move(value)) {}

    Result(const MediaError& error) : storage_(error) {}
    Result(MediaError&& error) : storage_(std::move(error)) {}

    Result(const Result& other) : storage_(other.storage_) {}
    Result(Result&& other) noexcept : storage_(std::move(other.storage_)) {}

    // ---- 赋值操作符 ----

    Result& operator=(const Result& other) {
        if (this != &other) {
            storage_ = other.storage_;
        }
        return *this;
    }

    Result& operator=(Result&& other) noexcept {
        if (this != &other) {
            storage_ = std::move(other.storage_);
        }
        return *this;
    }

    Result& operator=(const T& value) {
        storage_ = value;
        return *this;
    }

    Result& operator=(T&& value) {
        storage_ = std::move(value);
        return *this;
    }

    Result& operator=(const MediaError& error) {
        storage_ = error;
        return *this;
    }

    Result& operator=(MediaError&& error) {
        storage_ = std::move(error);
        return *this;
    }

    // ---- 状态查询 ----

    bool has_value() const noexcept { return storage_.which() == 0; }
    bool has_error() const noexcept { return storage_.which() == 1; }

    explicit operator bool() const noexcept { return has_value(); }
    bool operator!() const noexcept { return has_error(); }

    // ---- 值访问 ----

    T& value() {
        if (has_error()) {
            throw BadResultAccess(
                "Result does not contain a value: " +
                boost::get<MediaError>(storage_).full_message());
        }
        return boost::get<T>(storage_);
    }

    const T& value() const {
        if (has_error()) {
            throw BadResultAccess(
                "Result does not contain a value: " +
                boost::get<MediaError>(storage_).full_message());
        }
        return boost::get<T>(storage_);
    }

    T value_or(const T& default_value) const noexcept {
        return has_value() ? boost::get<T>(storage_) : default_value;
    }

    T value_or(T&& default_value) const noexcept {
        return has_value() ? boost::get<T>(storage_) : std::move(default_value);
    }

    T& operator*() { return value(); }
    const T& operator*() const { return value(); }
    T* operator->() { return &value(); }
    const T* operator->() const { return &value(); }

    // ---- 错误访问 ----

    MediaError& error() { return boost::get<MediaError>(storage_); }
    const MediaError& error() const { return boost::get<MediaError>(storage_); }

    // ---- 函数式操作 ----

    template<typename Func>
    auto map(Func&& func) const -> Result<decltype(func(std::declval<T>()))> {
        using U = decltype(func(std::declval<T>()));
        if (hasValue()) {
            return Result<U>(std::forward<Func>(func)(boost::get<T>(storage_)));
        }
        return Result<U>(error());
    }

    template<typename Func>
    auto and_then(Func&& func) const -> decltype(func(std::declval<T>())) {
        using ResultU = decltype(func(std::declval<T>()));
        if (has_value()) {
            return std::forward<Func>(func)(boost::get<T>(storage_));
        }
        return ResultU(error());
    }

    template<typename Func>
    Result or_else(Func&& func) const {
        if (has_value()) {
            return *this;
        }
        return std::forward<Func>(func)(error());
    }

private:
    // 索引0: T (值), 索引1: MediaError (错误)
    boost::variant<T, MediaError> storage_;
};

// ============================================================
// Result<void> 特化
// ============================================================
template<>
class Result<void> {
public:
    using ValueType = void;
    using ErrorType = MediaError;

    // ---- 构造函数 ----

    Result() : storage_(EmptyResult{}) {}

    Result(const MediaError& error) : storage_(error) {}
    Result(MediaError&& error) : storage_(std::move(error)) {}

    Result(const Result& other) : storage_(other.storage_) {}
    Result(Result&& other) noexcept : storage_(std::move(other.storage_)) {}

    // ---- 赋值操作符 ----

    Result& operator=(const Result& other) {
        if (this != &other) storage_ = other.storage_;
        return *this;
    }

    Result& operator=(Result&& other) noexcept {
        if (this != &other) storage_ = std::move(other.storage_);
        return *this;
    }

    Result& operator=(const MediaError& error) {
        storage_ = error;
        return *this;
    }

    Result& operator=(MediaError&& error) {
        storage_ = std::move(error);
        return *this;
    }

    // ---- 状态查询 ----

    bool has_value() const noexcept { return storage_.which() == 0; }
    bool has_error() const noexcept { return storage_.which() == 1; }

    explicit operator bool() const noexcept { return has_value(); }
    bool operator!() const noexcept { return has_error(); }

    // ---- 值访问 ----

    void value() const {
        if (has_error()) {
            throw BadResultAccess(
                "Result does not contain a value: " +
                boost::get<MediaError>(storage_).full_message());
        }
    }

    // ---- 错误访问 ----

    MediaError& error() { return boost::get<MediaError>(storage_); }
    const MediaError& error() const { return boost::get<MediaError>(storage_); }

    // ---- 函数式操作 ----

    template<typename Func>
    auto map(Func&& func) const -> Result<decltype(func())> {
        using U = decltype(func());
        if (has_value()) {
            return Result<U>(std::forward<Func>(func)());
        }
        return Result<U>(error());
    }

    template<typename Func>
    auto and_then(Func&& func) const -> decltype(func()) {
        if (has_value()) {
            return std::forward<Func>(func)();
        }
        return decltype(func())(error());
    }

    template<typename Func>
    Result or_else(Func&& func) const {
        if (has_value()) {
            return *this;
        }
        return std::forward<Func>(func)(error());
    }

private:
    boost::variant<EmptyResult, MediaError> storage_;
};

// ============================================================
// 工厂函数
// ============================================================

template<typename T>
Result<T> make_success(T&& value) {
    return Result<T>(std::forward<T>(value));
}

template<typename T>
Result<T> make_success(const T& value) {
    return Result<T>(value);
}

inline Result<void> make_success() {
    return Result<void>();
}

template<typename T>
Result<T> make_failure(const MediaError& error) {
    return Result<T>(error);
}

template<typename T>
Result<T> make_failure(MediaError&& error) {
    return Result<T>(std::move(error));
}

inline Result<void> make_failure(const MediaError& error) {
    return Result<void>(error);
}

inline Result<void> make_failure(MediaError&& error) {
    return Result<void>(std::move(error));
}

// ============================================================
// 便利宏
// ============================================================

#define CHECK_RESULT(expr) \
    do { \
        auto _checkResult = (expr); \
        if (_checkResult.has_error()) { \
            return _checkResult.error(); \
        } \
    } while(0)

#define ASSIGN_OR_RETURN(var, expr) \
    auto _assignResult = (expr); \
    if (_assignResult.has_error()) { \
        return _assignResult.error(); \
    } \
    var = std::move(_assignResult.value())

#define RETURN_IF_ERROR(expr) \
    do { \
        auto _returnResult = (expr); \
        if (_returnResult.has_error()) { \
            return _returnResult.error(); \
        } \
    } while(0)

#define TRY_OR_RETURN(expr) \
    do { \
        auto _tryResult = (expr); \
        if (_tryResult.has_error()) { \
            return _tryResult.error(); \
        } \
    } while(0)

} // namespace BridgePlayer