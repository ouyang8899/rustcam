#include "SystemUsageMonitor.h"

#include <QThread>
#include <QTimer>

namespace rustcam {

namespace {
constexpr int kSampleIntervalMs = 1000;
}

SystemUsageSampler::SystemUsageSampler(QObject* parent)
    : QObject(parent)
{
}

SystemUsageSampler::~SystemUsageSampler()
{
    delete m_probe;
}

void SystemUsageSampler::start()
{
    // 定时器必须在采样线程里创建，否则 timeout 会在界面线程派发。
    m_timer = new QTimer(this);
    m_timer->setInterval(kSampleIntervalMs);
    connect(m_timer, &QTimer::timeout, this, &SystemUsageSampler::sample);
    m_timer->start();
}

void SystemUsageSampler::sample()
{
    if (!m_probe) {
        // 探测器的初始化也要在采样线程里做（DXGI/PDH 的首次调用会阻塞）。
        m_probe = createSystemUsageProbe();
    }
    SystemUsage usage;
    if (m_probe) {
        usage.cpuValid = m_probe->sampleCpu(&usage.cpuPercent);
        usage.gpus = m_probe->sampleGpus();
        usage.memoryValid = m_probe->sampleMemory(&usage.memory);
    }
    emit usageChanged(usage);
}

SystemUsageMonitor::SystemUsageMonitor(QObject* parent)
    : QObject(parent)
{
    // 结果要跨线程投递，必须先注册类型，否则队列连接会静默失败。
    qRegisterMetaType<SystemUsage>("rustcam::SystemUsage");

    m_thread = new QThread(this);
    m_sampler = new SystemUsageSampler();
    m_sampler->moveToThread(m_thread);
    connect(m_thread, &QThread::started, m_sampler, &SystemUsageSampler::start);
    connect(m_sampler, &SystemUsageSampler::usageChanged,
            this, &SystemUsageMonitor::usageChanged);
    connect(m_thread, &QThread::finished, m_sampler, &QObject::deleteLater);
    m_thread->start();
}

SystemUsageMonitor::~SystemUsageMonitor()
{
    m_thread->quit();
    m_thread->wait();
}

} // namespace rustcam
