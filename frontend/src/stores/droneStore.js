// @ts-check
import { defineStore } from 'pinia'
import { ElMessage, ElNotification } from 'element-plus'
import * as api from '../api/drone'
import { useAuthStore } from './authStore'
import {
  isAlert,
  isAudit,
  isCommandReply,
  isDrone,
  isFence,
  isPayloadOf,
  isRoute,
  isTelemetry,
  isWsEnvelope,
} from '../utils/droneValidate'

const WS_KINDS = 'events'

export const useDroneStore = defineStore('drone', {
  state: () => ({
    drones: [],
    routes: [],
    fences: [],
    alerts: [],
    audits: [],
    telemetry: {},
    commands: {},
    controls: {},
    loading: false,
    wsConnected: false,
    ws: null,
    wsTimer: null,
    wsManuallyClosed: false,
    lastAlertTs: 0,
    /** 因校验不通过被丢弃的消息数（规则 §10.7）。 */
    droppedMessages: 0,
    /** WS 重连退避次数。 */
    wsRetry: 0,
    /** 控制权续约定时器：`{deviceId}:{type}` → timer。 */
    leaseTimers: {},
  }),

  getters: {
    onlineCount: (state) => state.drones.filter(d => d.online).length,
    getDrone: (state) => (deviceId) => state.drones.find(d => d.drone_tag === deviceId),
    getTelemetry: (state) => (deviceId) => state.telemetry[deviceId] || state.drones.find(d => d.drone_tag === deviceId)?.last_telemetry,
    getControl: (state) => (deviceId, type = 'flight') => state.controls[`${deviceId}:${type}`],
  },

  actions: {
    /**
     * 拉取无人机列表。
     * @param {{silent?: boolean}} [options] silent = true 时失败不弹提示。
     */
    async fetchDrones(options = {}) {
      this.loading = true
      try {
        const list = await api.getDrones(options)
        // REST 响应同样要校验后再进 store（规则 §10.7）。
        this.drones = Array.isArray(list) ? list.filter(isDrone) : []
        for (const d of this.drones) {
          if (d.last_telemetry && isTelemetry(d.last_telemetry)) {
            this.telemetry[d.drone_tag] = d.last_telemetry
          }
        }
      } finally {
        this.loading = false
      }
    },

    async saveDrone(drone) {
      const saved = drone.drone_tag && this.drones.some(d => d.drone_tag === drone.drone_tag)
        ? await api.updateDrone(drone.drone_tag, drone)
        : await api.createDrone(drone)
      await this.fetchDrones()
      ElMessage.success('无人机档案已保存')
      return saved
    },

    async removeDrone(deviceId) {
      await api.deleteDrone(deviceId)
      this.drones = this.drones.filter(d => d.drone_tag !== deviceId)
      ElMessage.success('无人机档案已删除')
    },

    async fetchDroneCameras(droneTag) {
      return api.getDroneCameras(droneTag)
    },

    async addDroneCamera(droneTag, cameraDeviceTag) {
      return api.addDroneCamera(droneTag, cameraDeviceTag)
    },

    async removeDroneCamera(droneTag, cameraDeviceTag) {
      return api.removeDroneCamera(droneTag, cameraDeviceTag)
    },

    /**
     * @param {string} deviceId
     * @param {{silent?: boolean}} [options] silent = true 时失败不弹提示。
     */
    async fetchRoutes(deviceId = '', options = {}) {
      const list = await api.getRoutes(deviceId, options)
      this.routes = Array.isArray(list) ? list.filter(isRoute) : []
    },

    async saveRoute(route) {
      const saved = route.id ? await api.updateRoute(route.id, route) : await api.createRoute(route)
      await this.fetchRoutes(route.drone_tag || '')
      ElMessage.success('航线已保存')
      return saved
    },

    async removeRoute(id) {
      await api.deleteRoute(id)
      this.routes = this.routes.filter(r => r.id !== id)
      ElMessage.success('航线已删除')
    },

    async executeRoute(id, deviceId = '', lateralOffsetM = 0) {
      const reply = await api.executeRoute(id, deviceId, lateralOffsetM)
      if (!this.validateCommandReply(reply)) return null
      this.commands[reply.cmd_id] = reply
      ElMessage.success(`航线已下发（${reply.status}）`)
      this.pollCommand(reply.cmd_id)
      return reply
    },

    /**
     * @param {{silent?: boolean}} [options] silent = true 时失败不弹提示。
     */
    async fetchFences(options = {}) {
      const list = await api.getFences(options)
      this.fences = Array.isArray(list) ? list.filter(isFence) : []
    },

    async saveFence(fence) {
      const saved = fence.id ? await api.updateFence(fence.id, fence) : await api.createFence(fence)
      await this.fetchFences()
      ElMessage.success('围栏已保存')
      return saved
    },

    async removeFence(id) {
      await api.deleteFence(id)
      this.fences = this.fences.filter(f => f.id !== id)
      ElMessage.success('围栏已删除')
    },

    async fetchAlerts(deviceId = '') {
      const list = await api.getAlerts(deviceId)
      this.alerts = Array.isArray(list) ? list.filter(isAlert) : []
    },

    async fetchAudits(deviceId = '') {
      const list = await api.getAudits(deviceId)
      this.audits = Array.isArray(list) ? list.filter(isAudit) : []
    },

    /**
     * 查询轨迹回放样本（规则 §10.7：REST 响应先校验再交给页面）。
     *
     * 服务端已按时间升序返回；这里再排序一次兜底，保证回放时间轴单调。
     * @param {string} deviceId
     * @param {number} startMs
     * @param {number} endMs
     * @param {number} [limit]
     * @returns {Promise<any[]>}
     */
    async fetchReplay(deviceId, startMs, endMs, limit = 5000) {
      const list = await api.getReplay(deviceId, startMs, endMs, limit)
      const raw = Array.isArray(list) ? list : []
      const samples = raw.filter(isTelemetry)
      this.droppedMessages += raw.length - samples.length
      return samples.sort((a, b) => a.ts_ms - b.ts_ms)
    },

    /**
     * 指令回执必须先通过运行时校验，再进入 store（规则 §10.7）。
     * @param {unknown} reply
     * @returns {boolean}
     */
    validateCommandReply(reply) {
      if (isCommandReply(reply)) return true
      this.droppedMessages += 1
      ElMessage.error('指令回执格式异常')
      return false
    },

    async sendCommand(deviceId, kind, params = {}) {
      let reply
      try {
        reply = await api.sendCommand(deviceId, { kind, params })
      } catch (e) {
        // 后端以 400/403/409 拒绝时，消息已经带在 error 里
        ElMessage.error(e?.message || '指令下发失败')
        throw e
      }
      if (!this.validateCommandReply(reply)) return null
      this.commands[reply.cmd_id] = reply
      if (reply.status === 'rejected') {
        ElMessage.error(reply.message || '指令被拒绝')
      } else {
        ElMessage.success(`已下发指令 ${kind}`)
      }
      this.pollCommand(reply.cmd_id)
      return reply
    },

    pollCommand(cmdId) {
      let tries = 0
      const timer = setInterval(async () => {
        tries++
        try {
          const status = await api.getCommand(cmdId)
          this.commands[cmdId] = { ...(this.commands[cmdId] || {}), ...status }
          if (['accepted', 'failed', 'timeout', 'rejected'].includes(status.status) || tries > 40) {
            clearInterval(timer)
          }
        } catch (e) {
          if (tries > 40) clearInterval(timer)
        }
      }, 500)
    },

    /**
     * 申请控制权。冲突（409）说明已有租约，此时刷新真实状态，
     * 不能拿 `held:false` 覆盖本地已知的持有人，否则界面会误显示“未持有”。
     */
    async acquireControl(deviceId, type = 'flight', priority = 10, leaseSeconds = 120) {
      const control = await api.acquireControl(deviceId, type, priority, leaseSeconds)
      const key = `${deviceId}:${type}`
      if (control.granted) {
        this.controls[key] = control
        ElMessage.success(`已获得${type === 'gimbal' ? '云台' : '飞行'}控制权`)
        this.startLeaseRenewal(deviceId, type, leaseSeconds)
        return control
      }
      const current = await this.fetchControl(deviceId, type)
      if (current?.granted) {
        ElMessage.info('你已持有控制权，无需重复申请')
        return current
      }
      ElMessage.warning(
        control.reason || (current?.holder ? `控制权由 ${current.holder} 持有` : '控制权被占用')
      )
      return control
    },

    /**
     * 租约续约：持有期间定期续约（规则 §6.7 / §6.13）。
     *
     * 租约到期后服务端会释放控制权，这里同步把本地状态置为未持有，
     * 避免界面显示"仍持有"但服务端已拒绝指令。
     */
    startLeaseRenewal(deviceId, type = 'flight', leaseSeconds = 120) {
      this.stopLeaseRenewal(deviceId, type)
      const key = `${deviceId}:${type}`
      const intervalMs = Math.max(10_000, Math.floor((leaseSeconds * 1000) / 3))
      const timer = setInterval(async () => {
        const control = this.controls[key]
        if (!control || !control.granted) {
          this.stopLeaseRenewal(deviceId, type)
          return
        }
        try {
          // 任务执行时后端可能把租约延长到覆盖整条航线；续约只允许“不减短”，
          // 否则前端会按申请时的短租约把服务端的长租约又缩回去。
          const remain = Math.max(
            0,
            Math.round((Number(control.expire_at_ms || 0) - Date.now()) / 1000)
          )
          const renewFor = Math.min(3600, Math.max(leaseSeconds, remain))
          const renewed = await api.renewControl(deviceId, type, renewFor)
          if (!renewed?.granted) {
            this.controls[key] = { ...control, ...renewed, granted: false, held: false }
            this.stopLeaseRenewal(deviceId, type)
            ElMessage.warning('控制权续约失败，可能已被他人接管')
            return
          }
          this.controls[key] = { ...control, ...renewed }
        } catch (e) {
          this.controls[key] = { ...control, granted: false, held: false }
          this.stopLeaseRenewal(deviceId, type)
          ElMessage.warning('控制权续约失败，可能已被他人接管')
        }
      }, intervalMs)
      this.leaseTimers = { ...this.leaseTimers, [key]: timer }
    },

    stopLeaseRenewal(deviceId, type = 'flight') {
      const key = `${deviceId}:${type}`
      const timer = this.leaseTimers[key]
      if (timer) {
        clearInterval(timer)
        const next = { ...this.leaseTimers }
        delete next[key]
        this.leaseTimers = next
      }
    },

    /**
     * 剩余租约秒数（规则 §10.7：控制权按钮必须显示剩余租约）。
     */
    leaseRemainSeconds(deviceId, type = 'flight') {
      const control = this.controls[`${deviceId}:${type}`]
      if (!control || !control.granted || !control.expire_at_ms) return 0
      return Math.max(0, Math.round((control.expire_at_ms - Date.now()) / 1000))
    },

    async releaseControl(deviceId, type = 'flight') {
      await api.releaseControl(deviceId, type)
      this.stopLeaseRenewal(deviceId, type)
      delete this.controls[`${deviceId}:${type}`]
      ElMessage.success('控制权已释放')
    },

    /**
     * 查询控制权状态。
     *
     * 页面刷新/切换无人机后，若租约仍属于当前用户，要恢复续约定时器——
     * 否则原租约到期后会被服务端回收，界面就会突然变成“未持有”。
     */
    async fetchControl(deviceId, type = 'flight') {
      try {
        const control = await api.getControl(deviceId, type)
        // 以“持有人是否为当前用户”为准，兼容旧版网关把任意租约都标成 granted。
        const me = useAuthStore().user?.username || ''
        const owned = me
          ? !!(control?.held && control.holder === me)
          : !!control?.granted
        const normalized = { ...control, granted: owned }
        this.controls[`${deviceId}:${type}`] = normalized
        if (owned && normalized.expire_at_ms) {
          const remain = Math.round((normalized.expire_at_ms - Date.now()) / 1000)
          if (remain > 5) {
            this.startLeaseRenewal(deviceId, type, Math.min(3600, remain))
          }
        }
        return normalized
      } catch (e) {
        return null
      }
    },

    async connectWs() {
      if (this.ws) return
      this.wsManuallyClosed = false
      try {
        const ticket = await api.getWsTicket()
        const proto = window.location.protocol === 'https:' ? 'wss' : 'ws'
        const url = `${proto}://${window.location.host}/ws/drone/${WS_KINDS}?ticket=${encodeURIComponent(ticket)}`
        const ws = new WebSocket(url)
        this.ws = ws
        ws.onopen = () => {
          this.wsConnected = true
          this.wsRetry = 0
        }
        ws.onmessage = (evt) => this.onWsMessage(evt)
        ws.onclose = () => {
          this.wsConnected = false
          this.ws = null
          if (!this.wsManuallyClosed) {
            this.scheduleWsReconnect()
          }
        }
        ws.onerror = () => {
          this.wsConnected = false
        }
      } catch (e) {
        this.scheduleWsReconnect()
      }
    },

    /**
     * 指数退避重连：1s → 30s，带 jitter（规则 §6.13 / §10.4）。
     */
    scheduleWsReconnect() {
      clearTimeout(this.wsTimer)
      const delay = Math.min(1000 * 2 ** this.wsRetry++, 30000) + Math.random() * 1000
      this.wsTimer = setTimeout(() => this.connectWs(), delay)
    },

    disconnectWs() {
      this.wsManuallyClosed = true
      clearTimeout(this.wsTimer)
      if (this.ws) {
        this.ws.close()
        this.ws = null
      }
      this.wsConnected = false
    },

    onWsMessage(evt) {
      let msg
      try {
        msg = JSON.parse(evt.data)
      } catch (e) {
        return
      }
      // 先校验信封与数据体，再进 store（规则 §10.7）。
      if (!isWsEnvelope(msg)) {
        this.droppedMessages += 1
        return
      }
      const data = msg.data
      if (!isPayloadOf(msg.msg_type, data)) {
        this.droppedMessages += 1
        return
      }
      if (msg.msg_type === 'telemetry') {
        const id = data.drone_tag
        if (!id) return
        this.telemetry[id] = data
        const idx = this.drones.findIndex(d => d.drone_tag === id)
        if (idx === -1) {
          this.drones.push({
            drone_tag: id,
            name: id,
            online: true,
            last_telemetry: data,
            autopilot_detected: data.autopilot,
            armed: data.armed,
            mode: data.mode,
            last_seen_ms: data.ts_ms,
          })
        } else {
          const d = this.drones[idx]
          d.online = true
          d.last_seen_ms = data.ts_ms
          d.last_telemetry = data
          // `autopilot` 是档案里配置的类型，遥测只能更新探测值。
          d.autopilot_detected = data.autopilot
          d.armed = data.armed
          d.mode = data.mode
        }
      } else if (msg.msg_type === 'alert') {
        this.alerts.unshift({
          drone_tag: data.drone_tag,
          rule: data.rule,
          level: data.level,
          message: data.message,
          ts_ms: msg.timestamp,
        })
        if (this.alerts.length > 200) this.alerts.pop()
        if (data.rule === 'geofence_breach' && msg.timestamp !== this.lastAlertTs) {
          this.lastAlertTs = msg.timestamp
          ElNotification({
            title: '围栏告警',
            message: `${data.drone_tag}: ${data.message}`,
            type: 'error',
            duration: 6000,
          })
        }
      } else if (msg.msg_type === 'command') {
        this.commands[data.cmd_id] = { ...(this.commands[data.cmd_id] || {}), ...data }
      } else if (msg.msg_type === 'control') {
        this.controls[`${data.drone_tag}:${data.control_type}`] = {
          ...(this.controls[`${data.drone_tag}:${data.control_type}`] || {}),
          ...data,
          held: data.action !== 'released' && data.action !== 'expired',
        }
        if (data.action === 'released' || data.action === 'expired') {
          this.stopLeaseRenewal(data.drone_tag, data.control_type)
        }
      }
    },
  },
})
