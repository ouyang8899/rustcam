#pragma once

#include <QObject>

#include "CoreExport.h"
#include "platform/SystemUsageProbe.h"

class QThread;
class QTimer;

namespace rustcam {

/// 采样线程里的工作对象：定时采集资源占用，结果通过信号送回主线程。
class RUSTCAM_CORE_API SystemUsageSampler : public QObject {
    Q_OBJECT
public:
    explicit SystemUsageSampler(QObject* parent = nullptr);
    ~SystemUsageSampler() override;

signals:
    void usageChanged(const rustcam::SystemUsage& usage);

public slots:
    /// 在采样线程里创建定时器并开始采样（由 QThread::started 触发）。
    void start();

private slots:
    void sample();

private:
    QTimer* m_timer = nullptr;
    SystemUsageProbe* m_probe = nullptr;
};

/// 采样本机 CPU / GPU / 内存占用，供主窗口状态栏显示。
///
/// 采样在独立线程里跑：PDH 枚举 GPU 引擎、DXGI 查询显卡名都会阻塞，
/// 放在界面线程会卡住重绘。结果以队列方式投递回主线程。
///
/// 口径与任务管理器一致：CPU 为系统整体占用，GPU 按 LUID 归并各引擎后取每块卡的最大值。
class RUSTCAM_CORE_API SystemUsageMonitor : public QObject {
    Q_OBJECT
public:
    explicit SystemUsageMonitor(QObject* parent = nullptr);
    ~SystemUsageMonitor() override;

signals:
    /// 每秒一次采样结果。
    void usageChanged(const rustcam::SystemUsage& usage);

private:
    QThread* m_thread = nullptr;
    SystemUsageSampler* m_sampler = nullptr;
};

} // namespace rustcam
