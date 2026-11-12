#include "SingleInstanceGuard.h"

#include <QSharedMemory>
#include <QtGlobal>

namespace rustcam {

SingleInstanceGuard::SingleInstanceGuard(qint64 schemeId)
{
    m_shared = new QSharedMemory(keyFor(schemeId));
}

SingleInstanceGuard::~SingleInstanceGuard()
{
    unlock();
    delete m_shared;
    m_shared = nullptr;
}

QString SingleInstanceGuard::keyFor(qint64 schemeId)
{
    return QStringLiteral("rustcam_video_tour_%1").arg(schemeId);
}

bool SingleInstanceGuard::tryLock()
{
    if (m_locked) {
        return true;
    }
    if (!m_shared) {
        return false;
    }
    if (m_shared->create(64)) {
        m_locked = true;
        return true;
    }
    if (m_shared->error() != QSharedMemory::AlreadyExists) {
        return false;
    }

#ifdef Q_OS_UNIX
    // POSIX 下进程崩溃遗留的共享内存段不会自动清理。先 attach/detach 一次：
    // 若原持有者仍存活，它的引用会保留该段；若已崩溃，最后引用释放后段被删除。
    if (m_shared->attach()) {
        m_shared->detach();
        if (m_shared->create(64)) {
            m_locked = true;
            return true;
        }
    }
#endif

    // 共享内存已存在：同机同方案已有实例在运行。
    return false;
}

void SingleInstanceGuard::unlock()
{
    if (m_locked && m_shared) {
        m_shared->detach();
    }
    m_locked = false;
}

} // namespace rustcam
