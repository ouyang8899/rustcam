#include "platform/SystemUsageProbe.h"

#include <QtGlobal>

#if !defined(Q_OS_WIN) && !defined(Q_OS_LINUX)
namespace rustcam {

/// 未适配的平台：取不到任何数据，上层显示 "--"。
class UnsupportedUsageProbe : public SystemUsageProbe {
public:
    bool sampleCpu(double* percent) override
    {
        Q_UNUSED(percent)
        return false;
    }

    QList<GpuUsage> sampleGpus() override
    {
        return QList<GpuUsage>();
    }

    bool sampleMemory(MemoryUsage* usage) override
    {
        Q_UNUSED(usage)
        return false;
    }
};

SystemUsageProbe* createSystemUsageProbe()
{
    return new UnsupportedUsageProbe();
}

} // namespace rustcam
#endif
