import { request } from '../utils/request'

export async function getChannels(params) {
  const res = await request.get('/channels', { params })
  return res.data.data
}

export async function getChannel(deviceTag, channelTag) {
  const res = await request.get(`/channels/${encodeURIComponent(deviceTag)}/${encodeURIComponent(channelTag)}`)
  return res.data.data
}

export async function getChannelPlayLinks(deviceTag, channelTag, params) {
  const res = await request.get(`/channels/${encodeURIComponent(deviceTag)}/${encodeURIComponent(channelTag)}/play-links`, { params })
  return res.data.data
}

export async function getChannelStatus(deviceTag, channelTag) {
  const res = await request.get(`/channels/${encodeURIComponent(deviceTag)}/${encodeURIComponent(channelTag)}/status`)
  return res.data.data
}

export async function getChannelPtzPresets(deviceTag, channelTag) {
  const res = await request.get(`/channels/${encodeURIComponent(deviceTag)}/${encodeURIComponent(channelTag)}/ptz/presets`)
  return res.data.data?.presets || []
}

export async function createChannelPtzPreset(deviceTag, channelTag, name) {
  const res = await request.post(`/channels/${encodeURIComponent(deviceTag)}/${encodeURIComponent(channelTag)}/ptz/presets`, { name })
  return res.data.data
}

export async function deleteChannelPtzPreset(deviceTag, channelTag, token) {
  const res = await request.delete(`/channels/${encodeURIComponent(deviceTag)}/${encodeURIComponent(channelTag)}/ptz/presets/${encodeURIComponent(token)}`)
  return res.data
}

export async function getChannelPtzStatus(deviceTag, channelTag) {
  const res = await request.get(`/channels/${encodeURIComponent(deviceTag)}/${encodeURIComponent(channelTag)}/ptz/status`)
  return res.data.data?.status || null
}

export async function channelPtzControl(deviceTag, channelTag, command, speed = 50, presetToken = null, extra = null) {
  const body = { speed }
  
  if (typeof command === 'string') {
    body.command = command
    if (presetToken) {
      body.move_type = 'goto_preset'
      body.preset_token = presetToken
    }
  } else if (command && command.type === 'continuous') {
    body.move_type = 'continuous'
    body.pan = command.pan
    body.tilt = command.tilt
    body.zoom = command.zoom
  } else if (command && command.type) {
    body.move_type = command.type
    if (command.preset_token) body.preset_token = command.preset_token
    if (command.preset_name) body.preset_name = command.preset_name
  }
  
  const res = await request.post(`/channels/${encodeURIComponent(deviceTag)}/${encodeURIComponent(channelTag)}/ptz`, body)
  return res.data
}
