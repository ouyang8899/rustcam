import { request } from '../utils/request'

/** 用户列表（分页）：返回 `{ items, total, limit, offset }` */
export async function getUsers(params = {}) {
  const res = await request.get('/users', { params })
  return res.data.data || { items: [], total: 0, limit: 0, offset: 0 }
}

export async function getUser(id) {
  const res = await request.get(`/users/${id}`)
  return res.data.data
}

export async function createUser(data) {
  const res = await request.post('/users', data)
  return res.data.data
}

export async function updateUser(id, data) {
  const res = await request.put(`/users/${id}`, data)
  return res.data.data
}

export async function deleteUser(id) {
  const res = await request.delete(`/users/${id}`)
  return res.data
}

export async function assignUserRoles(id, roleIds) {
  const res = await request.put(`/users/${id}/roles`, { role_ids: roleIds })
  return res.data
}
