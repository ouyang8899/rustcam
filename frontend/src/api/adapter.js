// @ts-check
import { request } from '../utils/request'

// 机载链路实体（adapter）：接入配置与启停
export async function getAdapters(includeDisabled = false) {
  const res = await request.get('/drone/adapters', {
    params: { include_disabled: includeDisabled },
  })
  return res.data.data
}

export async function getAdapter(adapterId) {
  const res = await request.get(`/drone/adapters/${encodeURIComponent(adapterId)}`)
  return res.data.data
}

export async function createAdapter(data) {
  const res = await request.post('/drone/adapters', data)
  return res.data.data
}

export async function updateAdapter(adapterId, data) {
  const res = await request.put(`/drone/adapters/${encodeURIComponent(adapterId)}`, data)
  return res.data.data
}

/** 停用 adapter（软删除）：配置与绑定关系保留，可恢复。 */
export async function disableAdapter(adapterId) {
  const res = await request.delete(`/drone/adapters/${encodeURIComponent(adapterId)}`)
  return res.data.data
}

export async function restoreAdapter(adapterId) {
  const res = await request.post(`/drone/adapters/${encodeURIComponent(adapterId)}/restore`)
  return res.data.data
}
