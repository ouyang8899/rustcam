import { request } from '../utils/request'

export async function getSystemConfig() {
  const res = await request.get('/system/config')
  return res.data.data || null
}

export async function getSystemConfigSecret() {
  const res = await request.get('/system/config/secret')
  return res.data.data || null
}
