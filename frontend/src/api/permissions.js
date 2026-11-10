import { request } from '../utils/request'

/** 权限点列表（分页）：返回 `{ items, total, limit, offset }` */
export async function getPermissions(params = {}) {
  const res = await request.get('/permissions', { params })
  return res.data.data || { items: [], total: 0, limit: 0, offset: 0 }
}

export async function createPermission(data) {
  const res = await request.post('/permissions', data)
  return res.data.data
}

export async function updatePermission(id, data) {
  const res = await request.put(`/permissions/${id}`, data)
  return res.data.data
}

export async function deletePermission(id) {
  const res = await request.delete(`/permissions/${id}`)
  return res.data
}
