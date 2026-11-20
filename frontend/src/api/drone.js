// @ts-check
import { request } from '../utils/request'

export async function getDrones(options = {}) {
  const res = await request.get('/drone/drones', options)
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

export async function getRoutes(deviceId = '', options = {}) {
  const res = await request.get('/drone/routes', { params: { drone_tag: deviceId }, ...options })
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
    { params: { drone_tag: deviceId, lateral_offset_m: lateralOffsetM } }
  )
  return res.data.data
}

export async function getFences(options = {}) {
  const res = await request.get('/drone/fences', options)
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

export async function sendCommand(deviceId, payload) {
  try {
    const res = await request.post(`/drone/${encodeURIComponent(deviceId)}/command`, payload)
    return res.data.data
  } catch (err) {
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

export async function getAlerts(deviceId = '', limit = 100) {
  const res = await request.get('/drone/alerts', { params: { drone_tag: deviceId, limit } })
  return res.data.data
}

export async function getAudits(deviceId = '', limit = 200) {
  const res = await request.get('/drone/audits', { params: { drone_tag: deviceId, limit } })
  return res.data.data
}

//
export async function getReplay(deviceId, startMs = 0, endMs = 0, limit = 5000) {
  const res = await request.get('/drone/replay', {
    params: {
      drone_tag: deviceId,
      start_ms: startMs,
      end_ms: endMs,
      limit,
    },
  })
  return res.data.data
}

//
function errorMessage(err, fallback) {
  return err?.response?.data?.message || err?.message || fallback
}

export async function acquireControl(deviceId, controlType = 'flight', priority = 10, leaseSeconds = 60) {
  try {
    const res = await request.post('/drone/control/acquire', {
      drone_tag: deviceId,
      control_type: controlType,
      priority,
      lease_seconds: leaseSeconds,
    })
    return res.data.data
  } catch (err) {
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
      drone_tag: deviceId,
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
    drone_tag: deviceId,
    control_type: controlType,
  })
  return res.data.data
}

export async function forceReleaseControl(deviceId, controlType = 'flight') {
  const res = await request.post('/drone/control/force_release', {
    drone_tag: deviceId,
    control_type: controlType,
  })
  return res.data.data
}

export async function renewControl(deviceId, controlType = 'flight', leaseSeconds = 60) {
  try {
    const res = await request.post('/drone/control/renew', {
      drone_tag: deviceId,
      control_type: controlType,
      lease_seconds: leaseSeconds,
    })
    return res.data.data
  } catch (err) {
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

export async function getWsTicket() {
  const res = await request.post('/drone/ws-ticket')
  return res.data.data.ticket
}
