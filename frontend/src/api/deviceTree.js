import { request } from '../utils/request'

export async function getDeviceTree(params = {}) {
  const query = {}
  if (params.search) query.search = params.search
  if (params.includeChannels === false) query.include_channels = false
  const res = await request.get('/device-tree', { params: query })
  return res.data.data || { nodes: [], region_nodes: [], stats: {} }
}
