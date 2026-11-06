// @ts-check
/**
 * 无人机消息的运行时校验（规则 §10.4 / §10.7）。
 *
 * 所有 WS 消息与 REST 响应在进入 store 之前都必须过一遍这里的校验，
 * 避免异常数据导致页面崩溃（规则 §16「前端消息未校验 → 页面崩溃」）。
 */

/**
 * @typedef {Object} Telemetry
 * @property {string} device_id
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
 * @property {string} device_id
 * @property {string} rule
 * @property {string} level
 * @property {string} message
 * @property {number} ts_ms
 */

/**
 * @typedef {Object} ControlState
 * @property {string} device_id
 * @property {string} control_type
 * @property {string} holder
 * @property {number} priority
 * @property {number} expire_at_ms
 * @property {string} action
 */

/**
 * @typedef {Object} CommandReply
 * @property {string} cmd_id
 * @property {string} device_id
 * @property {string} status
 * @property {string} message
 * @property {number} ts_ms
 */

/**
 * 是不是普通对象。
 * @param {unknown} value
 * @returns {value is Record<string, unknown>}
 */
function isObject(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

/**
 * 是不是有限数值。
 * @param {unknown} value
 * @returns {value is number}
 */
function isNumber(value) {
  return typeof value === 'number' && Number.isFinite(value)
}

/**
 * 是不是非空字符串。
 * @param {unknown} value
 * @returns {value is string}
 */
function isNonEmptyString(value) {
  return typeof value === 'string' && value.length > 0
}

/**
 * 遥测校验（规则 §10.4 的 isTelemetry）。
 * @param {unknown} value
 * @returns {boolean}
 */
export function isTelemetry(value) {
  if (!isObject(value)) return false
  return (
    isNonEmptyString(value.device_id) &&
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

/**
 * 告警校验。
 * @param {unknown} value
 * @returns {boolean}
 */
export function isAlert(value) {
  if (!isObject(value)) return false
  return (
    isNonEmptyString(value.device_id) &&
    isNonEmptyString(value.rule) &&
    isNonEmptyString(value.level) &&
    typeof value.message === 'string'
  )
}

/**
 * 控制权状态校验。
 * @param {unknown} value
 * @returns {boolean}
 */
export function isControl(value) {
  if (!isObject(value)) return false
  return (
    isNonEmptyString(value.device_id) &&
    isNonEmptyString(value.control_type) &&
    typeof value.holder === 'string' &&
    isNumber(value.priority) &&
    isNumber(value.expire_at_ms)
  )
}

/**
 * 指令回执校验。
 * @param {unknown} value
 * @returns {boolean}
 */
export function isCommandReply(value) {
  if (!isObject(value)) return false
  return (
    isNonEmptyString(value.cmd_id) &&
    isNonEmptyString(value.status) &&
    typeof value.message === 'string'
  )
}

/**
 * 审计记录校验。
 * @param {unknown} value
 * @returns {boolean}
 */
export function isAudit(value) {
  if (!isObject(value)) return false
  return (
    isNumber(value.ts_ms) &&
    isNonEmptyString(value.category) &&
    isNonEmptyString(value.action) &&
    typeof value.result === 'string'
  )
}

/**
 * 设备档案校验。
 * @param {unknown} value
 * @returns {boolean}
 */
export function isDrone(value) {
  if (!isObject(value)) return false
  return isNonEmptyString(value.device_id)
}

/**
 * 围栏校验。
 * @param {unknown} value
 * @returns {boolean}
 */
export function isFence(value) {
  if (!isObject(value)) return false
  const type = typeof value.type === 'string' ? value.type.toLowerCase() : ''
  return (
    isNonEmptyString(value.name) &&
    ['no_fly', 'work', 'height', 'allowed'].includes(type) &&
    typeof value.geojson === 'string'
  )
}

/**
 * 航线校验。
 * @param {unknown} value
 * @returns {boolean}
 */
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

/**
 * WS 信封校验：必须带 msg_type 与 data。
 * @param {unknown} value
 * @returns {boolean}
 */
export function isWsEnvelope(value) {
  if (!isObject(value)) return false
  return isNonEmptyString(value.msg_type) && isObject(value.data)
}

/**
 * 按 msg_type 校验数据体。
 * @param {string} msgType
 * @param {unknown} data
 * @returns {boolean}
 */
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
