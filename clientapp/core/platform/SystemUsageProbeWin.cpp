#include "platform/SystemUsageProbe.h"

#include <QtGlobal>

#ifdef Q_OS_WIN
#include <windows.h>
#include <pdh.h>
#include <PdhMsg.h>
#include <dxgi.h>

#include <QCoreApplication>
#include <QHash>
#include <QRegularExpression>
#include <algorithm>
#include <vector>

namespace rustcam {
namespace {

ULONG64 toUInt64(const FILETIME& value)
{
    return (static_cast<ULONG64>(value.dwHighDateTime) << 32) | value.dwLowDateTime;
}

QString luidKey(quint32 first, quint32 second)
{
    return QStringLiteral("%1_%2")
        .arg(first, 8, 16, QChar('0'))
        .arg(second, 8, 16, QChar('0'));
}

/// 从实例名里取出 LUID：pid_1234_luid_0x00000000_0x0000ABCD_phys_0_eng_0_engtype_3D
///
/// PDH 里两个 hex 的先后顺序各版本不一致，这里两种顺序都作为键。
bool parseLuidKeys(const QString& instanceName, QString* first, QString* second)
{
    static const QRegularExpression re(
        QStringLiteral("luid_0x([0-9a-fA-F]{1,8})_0x([0-9a-fA-F]{1,8})"));
    const QRegularExpressionMatch match = re.match(instanceName);
    if (!match.hasMatch()) {
        return false;
    }
    const quint32 high = match.captured(1).toUInt(nullptr, 16);
    const quint32 low = match.captured(2).toUInt(nullptr, 16);
    *first = luidKey(high, low);
    *second = luidKey(low, high);
    return true;
}

/// 显卡的静态信息：名称与显存总量。
struct AdapterInfo {
    QString name;
    quint64 dedicatedTotalBytes = 0;
    /// Microsoft Basic Render Driver：无硬件的软渲染，不显示。
    bool softwareRenderer = false;
};

/// 状态栏里的短名：去掉厂商前缀与 " Laptop GPU" 后缀。
QString shortAdapterName(const QString& name)
{
    static const QStringList prefixes = {
        QStringLiteral("NVIDIA GeForce "), QStringLiteral("NVIDIA Quadro "),
        QStringLiteral("NVIDIA "),        QStringLiteral("Intel(R) "),
        QStringLiteral("Intel "),         QStringLiteral("AMD Radeon "),
        QStringLiteral("AMD "),           QStringLiteral("Microsoft "),
        QStringLiteral("Apple "),         QStringLiteral("ATI "),
    };
    QString result = name;
    for (const QString& prefix : prefixes) {
        if (result.startsWith(prefix)) {
            result = result.mid(prefix.size());
            break;
        }
    }
    if (result.endsWith(QStringLiteral(" Laptop GPU"))) {
        result.chop(10);
    }
    return result;
}

/// DXGI 适配器表：LUID -> 名称 / 显存总量。
const QHash<QString, AdapterInfo>& adapterTable()
{
    static QHash<QString, AdapterInfo> table;
    static bool loaded = false;
    if (!loaded) {
        loaded = true;
        IDXGIFactory1* factory = nullptr;
        if (SUCCEEDED(::CreateDXGIFactory1(__uuidof(IDXGIFactory1),
                                          reinterpret_cast<void**>(&factory)))) {
            for (UINT index = 0;; ++index) {
                IDXGIAdapter1* adapter = nullptr;
                if (factory->EnumAdapters1(index, &adapter) == DXGI_ERROR_NOT_FOUND) {
                    break;
                }
                DXGI_ADAPTER_DESC desc;
                if (SUCCEEDED(adapter->GetDesc(&desc))) {
                    AdapterInfo info;
                    info.name = QString::fromWCharArray(desc.Description);
                    info.dedicatedTotalBytes = desc.DedicatedVideoMemory;
                    // 微软厂商 ID 的适配器是 Basic Render Driver 之类的软件渲染器。
                    info.softwareRenderer = desc.VendorId == 0x1414
                        || info.name.contains(QStringLiteral("Basic Render"));
                    const QString full = luidKey(
                        static_cast<quint32>(desc.AdapterLuid.HighPart), desc.AdapterLuid.LowPart);
                    const QString swapped = luidKey(
                        desc.AdapterLuid.LowPart, static_cast<quint32>(desc.AdapterLuid.HighPart));
                    table.insert(full, info);
                    table.insert(swapped, info);
                }
                adapter->Release();
            }
            factory->Release();
        }
    }
    return table;
}

/// 按 LUID 取显卡信息；两个键都试一遍，兼容 PDH 的 hex 顺序差异。
const AdapterInfo* adapterInfoFor(const QString& firstKey, const QString& secondKey)
{
    const QHash<QString, AdapterInfo>& table = adapterTable();
    auto it = table.constFind(firstKey);
    if (it != table.constEnd()) {
        return &it.value();
    }
    it = table.constFind(secondKey);
    return it != table.constEnd() ? &it.value() : nullptr;
}

/// 引擎类型：实例名末尾的 engtype_xxx。
enum class EngineKind {
    Render,   ///< 3D / Video / Compute / Copy
    Decode,   ///< Decode / VideoDecode
    Ignore
};

EngineKind engineKind(const QString& instanceName)
{
    static const QRegularExpression re(QStringLiteral("engtype_([A-Za-z0-9]+)"));
    const QString type = re.match(instanceName).captured(1).toLower();
    if (type == QLatin1String("decode") || type == QLatin1String("videodecode")) {
        return EngineKind::Decode;
    }
    if (type == QLatin1String("3d") || type == QLatin1String("video")
        || type == QLatin1String("compute") || type == QLatin1String("copy")) {
        return EngineKind::Render;
    }
    return EngineKind::Ignore;
}

/// 通用的 PDH 通配符计数器采样器。
class PdhWildcardSampler {
public:
    bool open(const wchar_t* counterPath)
    {
        if (::PdhOpenQueryW(nullptr, 0, &m_query) != ERROR_SUCCESS) {
            return false;
        }
        // 用英文计数器路径：本地化系统上的本地化路径不一致会取不到计数器。
        if (::PdhAddEnglishCounterW(m_query, counterPath, 0, &m_counter) != ERROR_SUCCESS) {
            ::PdhCloseQuery(m_query);
            m_query = nullptr;
            return false;
        }
        ::PdhCollectQueryData(m_query);
        return true;
    }

    /// 读出所有实例的当前值；失败返回空列表。
    QList<QPair<QString, double>> read()
    {
        QList<QPair<QString, double>> values;
        if (!m_query) {
            return values;
        }
        if (::PdhCollectQueryData(m_query) != ERROR_SUCCESS) {
            return values;
        }
        DWORD size = 0;
        DWORD count = 0;
        if (::PdhGetFormattedCounterArrayW(m_counter, PDH_FMT_DOUBLE, &size, &count, nullptr)
            != PDH_MORE_DATA) {
            return values;
        }
        if (size == 0) {
            return values;
        }
        std::vector<PDH_FMT_COUNTERVALUE_ITEM_W> items(
            size / sizeof(PDH_FMT_COUNTERVALUE_ITEM_W) + 1);
        if (::PdhGetFormattedCounterArrayW(m_counter, PDH_FMT_DOUBLE, &size, &count,
                                            items.data()) != ERROR_SUCCESS) {
            return values;
        }
        for (DWORD i = 0; i < count; ++i) {
            const PDH_FMT_COUNTERVALUE& value = items[i].FmtValue;
            if (value.CStatus != PDH_CSTATUS_VALID_DATA
                && value.CStatus != PDH_CSTATUS_NEW_DATA) {
                continue;
            }
            values.append(QPair<QString, double>(QString::fromWCharArray(items[i].szName),
                                                 value.doubleValue));
        }
        return values;
    }

    void close()
    {
        if (m_query) {
            ::PdhCloseQuery(m_query);
            m_query = nullptr;
        }
    }

private:
    PDH_HQUERY m_query = nullptr;
    PDH_HCOUNTER m_counter = nullptr;
};

class WindowsUsageProbe : public SystemUsageProbe {
public:
    WindowsUsageProbe()
    {
        FILETIME idle, kernel, user;
        if (queryTimes(&idle, &kernel, &user)) {
            m_lastIdle = toUInt64(idle);
            m_lastKernel = toUInt64(kernel);
            m_lastUser = toUInt64(user);
            m_cpuReady = true;
        }
        m_engineSampler.open(L"\\GPU Engine(*)\\Utilization Percentage");
        m_memorySampler.open(L"\\GPU Adapter Memory(*)\\Dedicated Usage");
    }

    bool sampleCpu(double* percent) override
    {
        FILETIME idle, kernel, user;
        if (!queryTimes(&idle, &kernel, &user)) {
            return false;
        }
        const ULONG64 nowIdle = toUInt64(idle);
        const ULONG64 nowKernel = toUInt64(kernel);
        const ULONG64 nowUser = toUInt64(user);
        if (!m_cpuReady) {
            m_lastIdle = nowIdle;
            m_lastKernel = nowKernel;
            m_lastUser = nowUser;
            m_cpuReady = true;
            return false;
        }
        const ULONG64 idleDelta = nowIdle - m_lastIdle;
        const ULONG64 totalDelta = (nowKernel - m_lastKernel) + (nowUser - m_lastUser);
        m_lastIdle = nowIdle;
        m_lastKernel = nowKernel;
        m_lastUser = nowUser;
        if (totalDelta == 0) {
            return false;
        }
        // kernel 时间含 idle，所以空闲占比用 idleDelta / totalDelta。
        const double busy = 1.0 - static_cast<double>(idleDelta) / static_cast<double>(totalDelta);
        *percent = busy * 100.0;
        return true;
    }

    QList<GpuUsage> sampleGpus() override
    {
        QList<GpuUsage> gpus;

        // 同一块卡的多个引擎（3D/Video/Compute）按 LUID 归到一起，取该卡的最大值。
        QHash<QString, GpuUsage> byAdapter;
        QHash<QString, const AdapterInfo*> infoByKey;
        const QList<QPair<QString, double>> engines = m_engineSampler.read();
        for (const QPair<QString, double>& engine : engines) {
            QString first;
            QString second;
            if (!parseLuidKeys(engine.first, &first, &second)) {
                continue;
            }
            const EngineKind kind = engineKind(engine.first);
            if (kind == EngineKind::Ignore) {
                continue;
            }
            GpuUsage& entry = byAdapter[first];
            if (kind == EngineKind::Decode) {
                entry.decodePercent = qMax(entry.decodePercent, engine.second);
                entry.decodeValid = true;
            } else {
                entry.renderPercent = qMax(entry.renderPercent, engine.second);
            }
            const AdapterInfo* info = adapterInfoFor(first, second);
            if (info) {
                entry.name = info->name;
                entry.memoryTotalBytes = info->dedicatedTotalBytes;
                infoByKey.insert(first, info);
            }
        }

        // 显存已用：\GPU Adapter Memory(*)\Dedicated Usage
        const QList<QPair<QString, double>> memory = m_memorySampler.read();
        for (const QPair<QString, double>& item : memory) {
            QString first;
            QString second;
            if (!parseLuidKeys(item.first, &first, &second)) {
                continue;
            }
            auto it = byAdapter.find(first);
            if (it == byAdapter.end()) {
                const AdapterInfo* info = adapterInfoFor(first, second);
                if (!info) {
                    continue;
                }
                GpuUsage entry;
                entry.name = info->name;
                entry.memoryTotalBytes = info->dedicatedTotalBytes;
                it = byAdapter.insert(first, entry);
                infoByKey.insert(first, info);
            }
            it->memoryUsedBytes = static_cast<quint64>(item.second);
            it->memoryValid = true;
        }

        for (auto it = byAdapter.constBegin(); it != byAdapter.constEnd(); ++it) {
            GpuUsage entry = it.value();
            const AdapterInfo* info = infoByKey.value(it.key(), nullptr);
            if (info && info->softwareRenderer) {
                continue;
            }
            entry.name = entry.name.isEmpty()
                ? QCoreApplication::translate("SystemUsageProbe", "GPU")
                : shortAdapterName(entry.name);
            gpus.append(entry);
        }
        // QHash 的遍历顺序每个进程都不同，按显存排序让状态栏顺序稳定：独显在前。
        std::stable_sort(gpus.begin(), gpus.end(),
                         [](const GpuUsage& left, const GpuUsage& right) {
                             return left.memoryTotalBytes > right.memoryTotalBytes;
                         });
        return gpus;
    }

    bool sampleMemory(MemoryUsage* usage) override
    {
        MEMORYSTATUSEX status;
        status.dwLength = sizeof(status);
        if (!::GlobalMemoryStatusEx(&status)) {
            return false;
        }
        if (status.ullTotalPhys == 0) {
            return false;
        }
        usage->totalBytes = status.ullTotalPhys;
        usage->usedBytes = status.ullTotalPhys - status.ullAvailPhys;
        usage->percent = static_cast<double>(usage->usedBytes) * 100.0
            / static_cast<double>(usage->totalBytes);
        return true;
    }

private:
    static bool queryTimes(FILETIME* idle, FILETIME* kernel, FILETIME* user)
    {
        return ::GetSystemTimes(idle, kernel, user) != FALSE;
    }

    PdhWildcardSampler m_engineSampler;
    PdhWildcardSampler m_memorySampler;
    bool m_cpuReady = false;
    ULONG64 m_lastIdle = 0;
    ULONG64 m_lastKernel = 0;
    ULONG64 m_lastUser = 0;
};

} // namespace

SystemUsageProbe* createSystemUsageProbe()
{
    return new WindowsUsageProbe();
}

} // namespace rustcam
#endif
