import { request } from '../utils/request'

export async function getDevices(params) {
  const res = await request.get('/devices', { params })
  return res.data.data
}

export async function getOnlineDevices(params) {
  const res = await request.get('/devices/online', { params })
  return res.data.data
}

export async function getDevice(deviceTag) {
  const res = await request.get(`/devices/${encodeURIComponent(deviceTag)}`)
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

export async function updateDeviceByTag(deviceTag, data) {
  const res = await request.put(`/devices/by-tag/${encodeURIComponent(deviceTag)}`, data)
  return res.data.data
}

export async function deleteDevice(deviceTag) {
  const res = await request.delete(`/devices/${encodeURIComponent(deviceTag)}`)
  return res.data
}

export async function playDevice(deviceTag) {
  const res = await request.post(`/devices/${encodeURIComponent(deviceTag)}/play`)
  return res.data.data
}

export async function startDevice(deviceTag, channelTag = '') {
  const params = channelTag ? `?channel_tag=${encodeURIComponent(channelTag)}` : ''
  const res = await request.post(`/devices/${encodeURIComponent(deviceTag)}/start${params}`)
  return res.data.data
}

export async function stopDevice(deviceTag) {
  const res = await request.post(`/devices/${encodeURIComponent(deviceTag)}/stop`)
  return res.data
}

export async function getPlayLinks(deviceTag) {
  const channelTag = deviceTag
  const res = await request.get(`/channels/${encodeURIComponent(deviceTag)}/${encodeURIComponent(channelTag)}/play-links`)
  return res.data.data
}

export async function getPtzPresets(deviceTag) {
  const channelTag = deviceTag
  const res = await request.get(`/channels/${encodeURIComponent(deviceTag)}/${encodeURIComponent(channelTag)}/ptz/presets`)
  return res.data.data?.presets || []
}

export async function createPtzPreset(deviceTag, name) {
  const channelTag = deviceTag
  const res = await request.post(`/channels/${encodeURIComponent(deviceTag)}/${encodeURIComponent(channelTag)}/ptz/presets`, { name })
  return res.data.data
}

export async function deletePtzPreset(deviceTag, token) {
  const channelTag = deviceTag
  const res = await request.delete(`/channels/${encodeURIComponent(deviceTag)}/${encodeURIComponent(channelTag)}/ptz/presets/${encodeURIComponent(token)}`)
  return res.data
}

export async function getPtzStatus(deviceTag) {
  const channelTag = deviceTag
  const res = await request.get(`/channels/${encodeURIComponent(deviceTag)}/${encodeURIComponent(channelTag)}/ptz/status`)
  return res.data.data?.status || null
}

export async function ptzControl(deviceTag, command, speed = 50, presetToken = null) {
  const channelTag = deviceTag
  const body = { command, speed }
  if (presetToken) {
    body.move_type = 'goto_preset'
    body.preset_token = presetToken
  }
  const res = await request.post(`/channels/${encodeURIComponent(deviceTag)}/${encodeURIComponent(channelTag)}/ptz`, body)
  return res.data
}

export async function startDevicePlayback(deviceTag, startTime, endTime) {
  const res = await request.post(`/devices/${encodeURIComponent(deviceTag)}/playback`, {
    start_time: startTime,
    end_time: endTime,
  })
  return res.data.data
}
