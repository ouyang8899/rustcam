// @ts-check
import { request } from '../utils/request'

// 无人机档案
export async function getDrones() {
  const res = await request.get('/drone/drones')
  return res.data.data
}

export async function getDrone(deviceId) {
  const res = await request.get(`/drone/drones/${encodeURIComponent(deviceId)}`)
  return res.data.data
}

export async function createDrone(data) {
  const res = await request.post('/drone/drones', data)
  return res.data.data
}

export async function updateDrone(deviceId, data) {
  const res = await request.put(`/drone/drones/${encodeURIComponent(deviceId)}`, data)
  return res.data.data
}

export async function deleteDrone(deviceId) {
  const res = await request.delete(`/drone/drones/${encodeURIComponent(deviceId)}`)
  return res.data.data
}

// 摄像头关联
export async function getDroneCameras(droneTag) {
  const res = await request.get(`/drone/drones/${encodeURIComponent(droneTag)}/cameras`)
  return res.data.data
}

export async function addDroneCamera(droneTag, cameraDeviceTag) {
  const res = await request.post(
    `/drone/drones/${encodeURIComponent(droneTag)}/cameras`,
    { camera_device_tag: cameraDeviceTag },
  )
  return res.data.data
}

export async function removeDroneCamera(droneTag, cameraDeviceTag) {
  const res = await request.delete(
    `/drone/drones/${encodeURIComponent(droneTag)}/cameras/${encodeURIComponent(cameraDeviceTag)}`,
  )
  return res.data.data
}

// 航线
export async function getRoutes(deviceId = '') {
  const res = await request.get('/drone/routes', { params: { device_id: deviceId } })
  return res.data.data
}

export async function createRoute(data) {
  const res = await request.post('/drone/routes', data)
  return res.data.data
}

export async function updateRoute(id, data) {
  const res = await request.put(`/drone/routes/${encodeURIComponent(id)}`, data)
  return res.data.data
}

export async function deleteRoute(id) {
  const res = await request.delete(`/drone/routes/${encodeURIComponent(id)}`)
  return res.data.data
}

export async function executeRoute(id, deviceId = '', lateralOffsetM = 0) {
  const res = await request.post(
    `/drone/routes/${encodeURIComponent(id)}/execute`,
    null,
    { params: { device_id: deviceId, lateral_offset_m: lateralOffsetM } }
  )
  return res.data.data
}

// 围栏
export async function getFences() {
  const res = await request.get('/drone/fences')
  return res.data.data
}

export async function createFence(data) {
  const res = await request.post('/drone/fences', data)
  return res.data.data
}

export async function updateFence(id, data) {
  const res = await request.put(`/drone/fences/${encodeURIComponent(id)}`, data)
  return res.data.data
}

export async function deleteFence(id) {
  const res = await request.delete(`/drone/fences/${encodeURIComponent(id)}`)
  return res.data.data
}

// 指令
export async function sendCommand(deviceId, payload) {
  try {
    const res = await request.post(`/drone/${encodeURIComponent(deviceId)}/command`, payload)
    return res.data.data
  } catch (err) {
    // 后端对参数越界/围栏拒绝/无控制权分别返回 400/403，消息要透出给操作员
    const error = new Error(
      err?.response?.data?.message || err?.message || '指令下发失败'
    )
    error.status = err?.response?.status || 0
    throw error
  }
}

export async function getCommand(cmdId) {
  const res = await request.get(`/drone/commands/${encodeURIComponent(cmdId)}`)
  return res.data.data
}

// 告警 / 审计
export async function getAlerts(deviceId = '', limit = 100) {
  const res = await request.get('/drone/alerts', { params: { device_id: deviceId, limit } })
  return res.data.data
}

export async function getAudits(deviceId = '', limit = 200) {
  const res = await request.get('/drone/audits', { params: { device_id: deviceId, limit } })
  return res.data.data
}

// 轨迹回放
//
// 返回指定时间窗内的遥测样本（服务端按 ts_ms 升序、超过 limit 时等步长降采样）。
// start_ms / end_ms 传 0 表示该侧不设限。
export async function getReplay(deviceId, startMs = 0, endMs = 0, limit = 5000) {
  const res = await request.get('/drone/replay', {
    params: {
      device_id: deviceId,
      start_ms: startMs,
      end_ms: endMs,
      limit,
    },
  })
  return res.data.data
}

// 控制权
//
// 后端用真实 HTTP 状态码表达失败（409 冲突 / 403 越权 / 503 服务不可用），
// 这里把错误信息整理出来交给调用方展示。
function errorMessage(err, fallback) {
  return err?.response?.data?.message || err?.message || fallback
}

export async function acquireControl(deviceId, controlType = 'flight', priority = 10, leaseSeconds = 60) {
  try {
    const res = await request.post('/drone/control/acquire', {
      device_id: deviceId,
      control_type: controlType,
      priority,
      lease_seconds: leaseSeconds,
    })
    return res.data.data
  } catch (err) {
    // 409：控制权已被他人持有
    return {
      granted: false,
      held: false,
      reason: errorMessage(err, '控制权不可用'),
    }
  }
}

export async function preemptControl(deviceId, controlType = 'flight', priority = 20, leaseSeconds = 60) {
  try {
    const res = await request.post('/drone/control/preempt', {
      device_id: deviceId,
      control_type: controlType,
      priority,
      lease_seconds: leaseSeconds,
    })
    return res.data.data
  } catch (err) {
    return {
      granted: false,
      held: false,
      reason: errorMessage(err, '抢占失败'),
    }
  }
}

export async function releaseControl(deviceId, controlType = 'flight') {
  const res = await request.post('/drone/control/release', {
    device_id: deviceId,
    control_type: controlType,
  })
  return res.data.data
}

export async function forceReleaseControl(deviceId, controlType = 'flight') {
  const res = await request.post('/drone/control/force_release', {
    device_id: deviceId,
    control_type: controlType,
  })
  return res.data.data
}

export async function renewControl(deviceId, controlType = 'flight', leaseSeconds = 60) {
  try {
    const res = await request.post('/drone/control/renew', {
      device_id: deviceId,
      control_type: controlType,
      lease_seconds: leaseSeconds,
    })
    return res.data.data
  } catch (err) {
    // 续约失败通常意味着控制权已被抢占或租约已过期
    return {
      granted: false,
      held: false,
      reason: errorMessage(err, '续约失败'),
    }
  }
}

export async function getControl(deviceId, controlType = 'flight') {
  const res = await request.get(`/drone/control/${encodeURIComponent(deviceId)}/${encodeURIComponent(controlType)}`)
  return res.data.data
}

// WebSocket 一次性票据
export async function getWsTicket() {
  const res = await request.post('/drone/ws-ticket')
  return res.data.data.ticket
}
