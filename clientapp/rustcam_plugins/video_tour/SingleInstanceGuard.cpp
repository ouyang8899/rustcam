#include "SingleInstanceGuard.h"

#include <QSharedMemory>

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
