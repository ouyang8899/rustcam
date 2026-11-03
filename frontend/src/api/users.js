import { request } from '../utils/request'

export async function getUsers() {
  const res = await request.get('/users')
  return res.data.data || []
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
