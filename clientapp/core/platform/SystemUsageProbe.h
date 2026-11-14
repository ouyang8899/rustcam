#pragma once

#include <QList>
#include <QMetaType>
#include <QString>

namespace rustcam {

/// 单块 GPU 的占用。
struct GpuUsage {
    QString name;
    /// 渲染占用百分比（0-100）：3D/Video/Compute 引擎。
    double renderPercent = 0.0;
    /// 解码占用百分比（0-100）：Decode/VideoDecode 引擎。
    double decodePercent = 0.0;
    /// 平台能区分解码引擎时为 true；为 false 时界面不显示解码一段。
    bool decodeValid = false;
    /// 显存已用与总量；memoryValid 为 false 表示取不到。
    bool memoryValid = false;
    quint64 memoryUsedBytes = 0;
    quint64 memoryTotalBytes = 0;
};

/// 物理内存占用。
struct MemoryUsage {
    double percent = 0.0;
    quint64 usedBytes = 0;
    quint64 totalBytes = 0;
};

/// 一次采样得到的全部占用数据。
struct SystemUsage {
    bool cpuValid = false;
    double cpuPercent = 0.0;
    QList<GpuUsage> gpus;
    bool memoryValid = false;
    MemoryUsage memory;
};

/// 平台相关的资源占用采样接口。
///
/// 各平台实现放在 platform/SystemUsageProbe*.cpp，由 CMake 按平台挑选；
/// 平台取不到的数据一律返回 false，由上层显示为 "--"。
class SystemUsageProbe {
public:
    virtual ~SystemUsageProbe() = default;

    /// 采样本机 CPU 占用百分比（0-100）；取不到返回 false。
    virtual bool sampleCpu(double* percent) = 0;
    /// 采集每块 GPU 的占用；取不到返回空列表。
    virtual QList<GpuUsage> sampleGpus() = 0;
    /// 采集物理内存占用；取不到返回 false。
    virtual bool sampleMemory(MemoryUsage* usage) = 0;
};

/// 创建当前平台的采样实现。
SystemUsageProbe* createSystemUsageProbe();

} // namespace rustcam

Q_DECLARE_METATYPE(rustcam::SystemUsage)
