// @ts-check

export function droneVideoAvailability(drone) {
  if (!drone) return { ok: false, reason: 'missing', cameras: [] }
  const cameras = Array.isArray(drone.camera_device_tags)
    ? drone.camera_device_tags.filter((tag) => typeof tag === 'string' && tag.length > 0)
    : []
  if (!drone.online) return { ok: false, reason: 'offline', cameras }
  if (cameras.length === 0) return { ok: false, reason: 'no_camera', cameras }
  return { ok: true, reason: '', cameras }
}

export function droneTagFromEntityId(id) {
  if (typeof id !== 'string' || !id.startsWith('drone-')) return ''
  return id.slice('drone-'.length)
}

export function pickPlayUrl(links) {
  if (!links || typeof links !== 'object') return ''
  return links.flv || links.web_flv || links.hls || links.webrtc || ''
}
