import { request } from './request'

function pickFlv(links) {
  if (!links) return null
  return links.web_flv || links.flv || null
}

async function findActiveStream(deviceTag) {
  const res = await request.get(`/streams/by-device/${deviceTag}`).catch(() => null)
  const streams = res?.data?.data || []
  return streams.find((s) => s.device_tag === deviceTag && s.state === 'Active') || null
}

export async function resolvePlayUrl(payload) {
  if (!payload?.deviceTag) return null
  const { deviceTag, channelTag } = payload
  try {
    if (channelTag) {
      const res = await request.get(`/channels/${deviceTag}/${channelTag}/play-links`).catch(() => null)
      const flv = pickFlv(res?.data?.data)
      if (flv) return flv
      if (channelTag !== deviceTag) {
        const res2 = await request.get(`/channels/${deviceTag}/${deviceTag}/play-links`).catch(() => null)
        const flv2 = pickFlv(res2?.data?.data)
        if (flv2) return flv2
      }
    } else {
      const res = await request.get(`/channels/${deviceTag}/${deviceTag}/play-links`).catch(() => null)
      const flv = pickFlv(res?.data?.data)
      if (flv) return flv
    }

    const channelsRes = await request.get(`/devices/${deviceTag}/channels`).catch(() => null)
    const channels = channelsRes?.data?.data || []
    if (channels.length > 0) {
      const firstTag = channels[0].channel_tag
      const res2 = await request.get(`/channels/${deviceTag}/${firstTag}/play-links`).catch(() => null)
      const flv2 = pickFlv(res2?.data?.data)
      if (flv2) return flv2
    }

    let stream = await findActiveStream(deviceTag)
    if (!stream) {
      try {
        await request.post(`/devices/${deviceTag}/play`)
        await new Promise((r) => setTimeout(r, 1000))
        stream = await findActiveStream(deviceTag)
      } catch {
        stream = null
      }
    }
    if (!stream) return null
    const flvRes = await request.get(`/streams/${stream.id}/play`, { params: { protocol: 'wsflv' } })
    return flvRes?.data?.data || null
  } catch {
    return null
  }
}
