import { request } from '../utils/request'

export async function getDevices(params) {
  const res = await request.get('/devices', { params })
  return res.data.data
}

export async function getOnlineDevices(params) {
  const res = await request.get('/devices/online', { params })
  return res.data.data
}

export async function getDevice(id) {
  const res = await request.get(`/devices/${id}`)
  return res.data.data
}

export async function createDevice(data) {
  const res = await request.post('/devices', data)
  return res.data.data
}

export async function updateDevice(id, data) {
  const res = await request.put(`/devices/${id}`, data)
  return res.data.data
}

export async function deleteDevice(id) {
  const res = await request.delete(`/devices/${id}`)
  return res.data
}

export async function playDevice(id) {
  const res = await request.post(`/devices/${id}/play`)
  return res.data.data
}

export async function startDevice(id) {
  const res = await request.post(`/devices/${id}/start`)
  return res.data.data
}

export async function stopDevice(id) {
  const res = await request.post(`/devices/${id}/stop`)
  return res.data
}

export async function getPlayLinks(id) {
  const res = await request.get(`/devices/${id}/play-links`)
  return res.data.data
}

export async function getPtzPresets(id) {
  const res = await request.get(`/devices/${id}/ptz/presets`)
  return res.data.data?.presets || []
}

export async function createPtzPreset(id, name) {
  const res = await request.post(`/devices/${id}/ptz/presets`, { name })
  return res.data.data
}

export async function deletePtzPreset(id, token) {
  const res = await request.delete(`/devices/${id}/ptz/presets/${token}`)
  return res.data
}

export async function getPtzStatus(id) {
  const res = await request.get(`/devices/${id}/ptz/status`)
  return res.data.data?.status || null
}

export async function ptzControl(id, command, speed = 50, presetToken = null) {
  const body = { command, speed }
  if (presetToken) {
    body.move_type = 'goto_preset'
    body.preset_token = presetToken
  }
  const res = await request.post(`/devices/${id}/ptz`, body)
  return res.data
}

export async function startDevicePlayback(id, startTime, endTime) {
  const res = await request.post(`/devices/${id}/playback`, {
    start_time: startTime,
    end_time: endTime,
  })
  return res.data.data
}
