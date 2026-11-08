// @ts-check

/**
 * 判断点击无人机时是否应弹出实时视频浮窗。
 * 只有在线且至少关联一个摄像头时才允许。
 */
export function droneVideoAvailability(drone) {
  if (!drone) return { ok: false, reason: 'missing', cameras: [] }
  const cameras = Array.isArray(drone.camera_device_tags)
    ? drone.camera_device_tags.filter((tag) => typeof tag === 'string' && tag.length > 0)
    : []
  if (!drone.online) return { ok: false, reason: 'offline', cameras }
  if (cameras.length === 0) return { ok: false, reason: 'no_camera', cameras }
  return { ok: true, reason: '', cameras }
}

/**
 * 从 Cesium 无人机实体 id（drone-<drone_tag>）解析 drone_tag。
 */
export function droneTagFromEntityId(id) {
  if (typeof id !== 'string' || !id.startsWith('drone-')) return ''
  return id.slice('drone-'.length)
}

/**
 * 从平台 play-links 响应中选择可播放地址。
 */
export function pickPlayUrl(links) {
  if (!links || typeof links !== 'object') return ''
  return links.flv || links.web_flv || links.hls || links.webrtc || ''
}
