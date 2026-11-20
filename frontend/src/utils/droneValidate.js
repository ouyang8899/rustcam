// @ts-check

/**
 * @typedef {Object} Telemetry
 * @property {string} drone_tag
 * @property {number} ts_ms
 * @property {number} lat
 * @property {number} lon
 * @property {number} alt_amsl
 * @property {number} rel_alt
 * @property {number} roll
 * @property {number} pitch
 * @property {number} yaw
 * @property {number} battery_pct
 * @property {number} link_quality
 * @property {number} seq
 */

/**
 * @typedef {Object} Alert
 * @property {string} drone_tag
 * @property {string} rule
 * @property {string} level
 * @property {string} message
 * @property {number} ts_ms
 */

/**
 * @typedef {Object} ControlState
 * @property {string} drone_tag
 * @property {string} control_type
 * @property {string} holder
 * @property {number} priority
 * @property {number} expire_at_ms
 * @property {string} action
 */

/**
 * @typedef {Object} CommandReply
 * @property {string} cmd_id
 * @property {string} drone_tag
 * @property {string} status
 * @property {string} message
 * @property {number} ts_ms
 */

function isObject(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function isNumber(value) {
  return typeof value === 'number' && Number.isFinite(value)
}

function isNonEmptyString(value) {
  return typeof value === 'string' && value.length > 0
}

export function isTelemetry(value) {
  if (!isObject(value)) return false
  return (
    isNonEmptyString(value.drone_tag) &&
    isNumber(value.ts_ms) &&
    isNumber(value.lat) &&
    isNumber(value.lon) &&
    isNumber(value.alt_amsl) &&
    isNumber(value.rel_alt) &&
    isNumber(value.roll) &&
    isNumber(value.pitch) &&
    isNumber(value.yaw) &&
    isNumber(value.battery_pct) &&
    isNumber(value.link_quality) &&
    isNumber(value.seq)
  )
}

export function isAlert(value) {
  if (!isObject(value)) return false
  return (
    isNonEmptyString(value.drone_tag) &&
    isNonEmptyString(value.rule) &&
    isNonEmptyString(value.level) &&
    typeof value.message === 'string'
  )
}

export function isControl(value) {
  if (!isObject(value)) return false
  return (
    isNonEmptyString(value.drone_tag) &&
    isNonEmptyString(value.control_type) &&
    typeof value.holder === 'string' &&
    isNumber(value.priority) &&
    isNumber(value.expire_at_ms)
  )
}

export function isCommandReply(value) {
  if (!isObject(value)) return false
  return (
    isNonEmptyString(value.cmd_id) &&
    isNonEmptyString(value.status) &&
    typeof value.message === 'string'
  )
}

export function isAudit(value) {
  if (!isObject(value)) return false
  return (
    isNumber(value.ts_ms) &&
    isNonEmptyString(value.category) &&
    isNonEmptyString(value.action) &&
    typeof value.result === 'string'
  )
}

export function isDrone(value) {
  if (!isObject(value)) return false
  return isNonEmptyString(value.drone_tag)
}

export function isFence(value) {
  if (!isObject(value)) return false
  const type = typeof value.type === 'string' ? value.type.toLowerCase() : ''
  return (
    isNonEmptyString(value.name) &&
    ['no_fly', 'work', 'height', 'allowed'].includes(type) &&
    typeof value.geojson === 'string'
  )
}

export function isRoute(value) {
  if (!isObject(value)) return false
  return (
    isNonEmptyString(value.id) &&
    Array.isArray(value.waypoints) &&
    value.waypoints.every(
      (/** @type {any} */ wp) =>
        isObject(wp) && isNumber(wp.lat) && isNumber(wp.lon) && isNumber(wp.alt)
    )
  )
}

export function isWsEnvelope(value) {
  if (!isObject(value)) return false
  return isNonEmptyString(value.msg_type) && isObject(value.data)
}

export function isPayloadOf(msgType, data) {
  switch (msgType) {
    case 'telemetry':
      return isTelemetry(data)
    case 'alert':
      return isAlert(data)
    case 'control':
      return isControl(data)
    case 'command':
      return isCommandReply(data)
    default:
      return false
  }
}
