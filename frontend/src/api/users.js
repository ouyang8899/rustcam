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

/**
 * 设置超级用户标志。
 *
 * 只有超级用户能改这个标志（服务端会实时复核）；变更后该用户的会话会被吊销，
 * 需要重新登录才会生效。
 */
export async function setUserSuper(id, isSuper) {
  const res = await request.put(`/users/${id}/super`, { is_super: isSuper })
  return res.data
}
