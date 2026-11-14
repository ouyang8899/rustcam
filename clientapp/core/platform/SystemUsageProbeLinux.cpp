#include "platform/SystemUsageProbe.h"

#include <QtGlobal>

#ifdef Q_OS_LINUX
#include <QDir>
#include <QFile>
#include <QRegularExpression>
#include <QStringList>

namespace rustcam {
namespace {

/// 读一个只含一个整数的 sysfs 文件。
bool readUInt64File(const QString& path, quint64* value)
{
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly)) {
        return false;
    }
    bool ok = false;
    const quint64 parsed = file.readAll().trimmed().toULongLong(&ok);
    if (ok) {
        *value = parsed;
    }
    return ok;
}

/// 按 PCI 厂商 ID 给出显卡短名，取不到返回空。
QString vendorName(const QString& devicePath)
{
    QFile file(devicePath + QStringLiteral("/vendor"));
    if (!file.open(QIODevice::ReadOnly)) {
        return QString();
    }
    const QString vendor = file.readAll().trimmed().toLower();
    if (vendor == QLatin1String("0x10de")) {
        return QStringLiteral("NVIDIA");
    }
    if (vendor == QLatin1String("0x1002")) {
        return QStringLiteral("AMD");
    }
    if (vendor == QLatin1String("0x8086")) {
        return QStringLiteral("Intel");
    }
    if (vendor == QLatin1String("0x13b5")) {
        return QStringLiteral("ARM");
    }
    return QString();
}

/// /proc/stat 聚合行：cpu user nice system idle iowait irq softirq steal ...
bool readCpuTimes(quint64* idle, quint64* total)
{
    QFile file(QStringLiteral("/proc/stat"));
    if (!file.open(QIODevice::ReadOnly)) {
        return false;
    }
    const QByteArray line = file.readLine();
    const QList<QByteArray> parts = line.simplified().split(' ');
    if (parts.size() < 5 || parts.first() != "cpu") {
        return false;
    }
    quint64 sum = 0;
    quint64 idleValue = 0;
    for (int i = 1; i < parts.size(); ++i) {
        bool ok = false;
        const quint64 value = parts.at(i).toULongLong(&ok);
        if (!ok) {
            return false;
        }
        sum += value;
        // idle 与 iowait 都是空闲时间。
        if (i == 4 || i == 5) {
            idleValue += value;
        }
    }
    *idle = idleValue;
    *total = sum;
    return true;
}

bool readMemInfo(quint64* totalKb, quint64* availableKb)
{
    QFile file(QStringLiteral("/proc/meminfo"));
    if (!file.open(QIODevice::ReadOnly)) {
        return false;
    }
    *totalKb = 0;
    *availableKb = 0;
    while (!file.atEnd()) {
        const QByteArray line = file.readLine();
        if (line.startsWith("MemTotal:")) {
            *totalKb = line.split(':').value(1).trimmed().split(' ').value(0).toULongLong();
        } else if (line.startsWith("MemAvailable:")) {
            *availableKb = line.split(':').value(1).trimmed().split(' ').value(0).toULongLong();
        }
    }
    return *totalKb > 0;
}

class LinuxUsageProbe : public SystemUsageProbe {
public:
    bool sampleCpu(double* percent) override
    {
        quint64 idle = 0;
        quint64 total = 0;
        if (!readCpuTimes(&idle, &total)) {
            return false;
        }
        if (!m_ready) {
            m_lastIdle = idle;
            m_lastTotal = total;
            m_ready = true;
            return false;
        }
        const quint64 idleDelta = idle - m_lastIdle;
        const quint64 totalDelta = total - m_lastTotal;
        m_lastIdle = idle;
        m_lastTotal = total;
        if (totalDelta == 0) {
            return false;
        }
        const double busy = 1.0 - static_cast<double>(idleDelta) / static_cast<double>(totalDelta);
        *percent = busy * 100.0;
        return true;
    }

    QList<GpuUsage> sampleGpus() override
    {
        // 只有部分驱动暴露忙碌百分比（amdgpu 的 gpu_busy_percent），
        // 取不到的卡（如 NVIDIA 闭源驱动）不显示，避免给出假数据。
        QList<GpuUsage> gpus;
        QDir dir(QStringLiteral("/sys/class/drm"));
        const QStringList cards = dir.entryList(QStringList() << QStringLiteral("card*"),
                                                 QDir::Dirs | QDir::NoDotAndDotDot);
        static const QRegularExpression cardPattern(QStringLiteral("^card(\\d+)$"));
        for (const QString& card : cards) {
            const QRegularExpressionMatch match = cardPattern.match(card);
            if (!match.hasMatch()) {
                // card0-DP-1 之类的连接器目录，不是显卡本身。
                continue;
            }
            const QString devicePath = dir.filePath(card) + QStringLiteral("/device");
            QFile file(devicePath + QStringLiteral("/gpu_busy_percent"));
            if (!file.open(QIODevice::ReadOnly)) {
                continue;
            }
            bool ok = false;
            const double value = file.readAll().trimmed().toDouble(&ok);
            if (!ok || value < 0.0) {
                continue;
            }

            GpuUsage usage;
            const QString vendor = vendorName(devicePath);
            usage.name = vendor.isEmpty()
                ? QStringLiteral("GPU %1").arg(match.captured(1))
                : QStringLiteral("%1 GPU %2").arg(vendor, match.captured(1));
            usage.renderPercent = value;

            quint64 vramUsed = 0;
            quint64 vramTotal = 0;
            if (readUInt64File(devicePath + QStringLiteral("/mem_info_vram_used"), &vramUsed)
                && readUInt64File(devicePath + QStringLiteral("/mem_info_vram_total"), &vramTotal)
                && vramTotal > 0) {
                usage.memoryValid = true;
                usage.memoryUsedBytes = vramUsed;
                usage.memoryTotalBytes = vramTotal;
            }
            gpus.append(usage);
        }
        return gpus;
    }

    bool sampleMemory(MemoryUsage* usage) override
    {
        quint64 totalKb = 0;
        quint64 availableKb = 0;
        if (!readMemInfo(&totalKb, &availableKb)) {
            return false;
        }
        usage->totalBytes = totalKb * 1024ULL;
        usage->usedBytes = (totalKb > availableKb ? totalKb - availableKb : 0) * 1024ULL;
        usage->percent = static_cast<double>(usage->usedBytes) * 100.0
            / static_cast<double>(usage->totalBytes);
        return true;
    }

private:
    bool m_ready = false;
    quint64 m_lastIdle = 0;
    quint64 m_lastTotal = 0;
};

} // namespace

SystemUsageProbe* createSystemUsageProbe()
{
    return new LinuxUsageProbe();
}

} // namespace rustcam
#endif
