import { request } from '../utils/request'

export async function getPermissions() {
  const res = await request.get('/permissions')
  return res.data.data || []
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
