import { request } from '../utils/request'

export async function getRoles() {
  const res = await request.get('/roles')
  return res.data.data || []
}

export async function getRole(id) {
  const res = await request.get(`/roles/${id}`)
  return res.data.data
}

export async function createRole(data) {
  const res = await request.post('/roles', data)
  return res.data.data
}

export async function updateRole(id, data) {
  const res = await request.put(`/roles/${id}`, data)
  return res.data.data
}

export async function deleteRole(id) {
  const res = await request.delete(`/roles/${id}`)
  return res.data
}

export async function setRolePermissions(id, permissionIds) {
  const res = await request.put(`/roles/${id}/permissions`, { permission_ids: permissionIds })
  return res.data
}

export async function setRoleParents(id, parentRoleIds) {
  const res = await request.put(`/roles/${id}/parents`, { parent_role_ids: parentRoleIds })
  return res.data.data || []
}
