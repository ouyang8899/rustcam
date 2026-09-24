#pragma once
#include <vector>
#include <atomic>
#include <cstddef>

// 无锁环形缓冲区（单生产者单消费者）
namespace BridgePlayer {
    template<typename T>
    class SPSCRingBuffer {
    public:
        explicit SPSCRingBuffer(size_t capacity)
            : capacity_(capacity), buffer_(capacity) {}

        bool push(T&& item) {
            size_t write = write_pos_.load(std::memory_order_relaxed);
            size_t next = (write + 1) % capacity_;
            if (next == read_pos_.load(std::memory_order_acquire)) {
                return false;  // 满
            }
            buffer_[write] = std::move(item);
            write_pos_.store(next, std::memory_order_release);
            return true;
        }

        bool pop(T& item) {
            size_t read = read_pos_.load(std::memory_order_relaxed);
            if (read == write_pos_.load(std::memory_order_acquire)) {
                return false;  // 空
            }
            item = std::move(buffer_[read]);
            read_pos_.store((read + 1) % capacity_, std::memory_order_release);
            return true;
        }

        size_t size() const {
            size_t write = write_pos_.load(std::memory_order_acquire);
            size_t read = read_pos_.load(std::memory_order_acquire);
            return (write >= read) ? write - read : capacity_ - read + write;
        }

    private:
        size_t capacity_;
        std::vector<T> buffer_;
        std::atomic<size_t> write_pos_{ 0 };
        std::atomic<size_t> read_pos_{ 0 };
    };
}
