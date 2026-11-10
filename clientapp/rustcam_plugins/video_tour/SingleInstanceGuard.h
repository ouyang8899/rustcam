#pragma once

#include <QString>

class QSharedMemory;

namespace rustcam {

/// 同机同方案的互斥锁：同名共享内存存在即表示已有实例在跑。
class SingleInstanceGuard {
public:
    explicit SingleInstanceGuard(qint64 schemeId);
    ~SingleInstanceGuard();

    /// 尝试加锁；失败说明同方案已在另一实例运行。
    bool tryLock();
    void unlock();
    bool locked() const { return m_locked; }

    static QString keyFor(qint64 schemeId);

private:
    QSharedMemory* m_shared = nullptr;
    bool m_locked = false;
};

} // namespace rustcam
