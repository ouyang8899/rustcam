export const DRAG_MIME = 'application/x-rustcam-node'

export function buildDragPayload(node) {
  if (!node) return null
  if (node.type === 'channel') {
    return {
      type: 'channel',
      deviceTag: node.device_tag,
      channelTag: node.channel_tag,
      name: node.name || node.channel_tag,
      status: node.status || null,
      protocol: node.protocol || null,
      groupId: node.group_id ?? null,
    }
  }
  if (node.type === 'device') {
    return {
      type: 'device',
      deviceTag: node.device_tag,
      channelTag: null,
      name: node.name || node.device_tag,
      status: node.status || null,
      protocol: node.protocol || null,
      groupId: node.group_id ?? null,
    }
  }
  return null
}

export function readDragPayload(event) {
  const dt = event?.dataTransfer
  if (!dt) return null
  let raw = ''
  try {
    raw = dt.getData(DRAG_MIME) || dt.getData('text/plain') || ''
  } catch {
    raw = ''
  }
  if (!raw) return null
  try {
    const parsed = JSON.parse(raw)
    if (!parsed || !parsed.deviceTag) return null
    return parsed
  } catch {
    return null
  }
}
