import { request, setTokens, clearTokens } from '../utils/request'

export async function login(username, password) {
  const res = await request.post('/auth/login', {
    username,
    password,
    client_type: 'web',
    client_name: navigator.userAgent,
  })
  const data = res.data.data
  setTokens(data.access_token, data.refresh_token)
  localStorage.setItem('rustcam_user', JSON.stringify(data.user))
  return data.user
}

export async function refreshToken() {
  const refresh_token = localStorage.getItem('rustcam_refresh_token')
  if (!refresh_token) return false
  try {
    const res = await request.post('/auth/refresh', { refresh_token })
    const data = res.data.data
    setTokens(data.access_token, data.refresh_token)
    localStorage.setItem('rustcam_user', JSON.stringify(data.user))
    return true
  } catch {
    clearTokens()
    return false
  }
}

export async function getCurrentUser() {
  const res = await request.get('/auth/me')
  return res.data.data
}

export async function logout() {
  try {
    await request.post('/auth/logout')
  } catch {
  }
  clearTokens()
}

export async function getMySessions(params = {}) {
  const res = await request.get('/auth/sessions', { params })
  return res.data.data || { items: [], total: 0, limit: 0, offset: 0 }
}

export async function getAllSessions(params = {}) {
  const res = await request.get('/auth/sessions/all', { params })
  return res.data.data || { items: [], total: 0, limit: 0, offset: 0 }
}

export async function revokeSession(id) {
  const res = await request.delete(`/auth/sessions/${id}`)
  return res.data
}
