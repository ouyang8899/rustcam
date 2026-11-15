#include "CellChannelRegistry.h"

namespace rustcam {

void CellChannelRegistry::set(const QString& cellId, const QString& deviceTag, const QString& channelTag)
{
    if (cellId.isEmpty() || deviceTag.isEmpty() || channelTag.isEmpty()) {
        remove(cellId);
        return;
    }

    Entry entry;
    entry.deviceTag = deviceTag;
    entry.channelTag = channelTag;
    m_entries.insert(cellId, entry);
}

void CellChannelRegistry::remove(const QString& cellId)
{
    m_entries.remove(cellId);
}

void CellChannelRegistry::swapCells(const QString& firstCellId, const QString& secondCellId)
{
    if (firstCellId.isEmpty() || secondCellId.isEmpty() || firstCellId == secondCellId) {
        return;
    }

    const Entry first = m_entries.value(firstCellId);
    const Entry second = m_entries.value(secondCellId);

    m_entries.remove(firstCellId);
    m_entries.remove(secondCellId);

    if (!second.deviceTag.isEmpty() && !second.channelTag.isEmpty()) {
        m_entries.insert(firstCellId, second);
    }
    if (!first.deviceTag.isEmpty() && !first.channelTag.isEmpty()) {
        m_entries.insert(secondCellId, first);
    }
}

bool CellChannelRegistry::channelOf(const QString& cellId, QString* deviceTag, QString* channelTag) const
{
    const auto it = m_entries.constFind(cellId);
    if (it == m_entries.constEnd()) {
        return false;
    }

    if (deviceTag) {
        *deviceTag = it->deviceTag;
    }
    if (channelTag) {
        *channelTag = it->channelTag;
    }
    return true;
}

void CellChannelRegistry::clear()
{
    m_entries.clear();
}

}
