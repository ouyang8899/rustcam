import { ref } from 'vue'
import { request, clearTokens } from '../utils/request'

const wsConnected = ref(false)
let ws = null
let reconnectTimer = null
let reconnectDelay = 1000
let manualClose = false
const subscribers = new Set()

async function fetchTicket() {
  const res = await request.post('/ws/ticket')
  return res.data.data.ticket
}

function notify(raw) {
  let msg
  try {
    msg = JSON.parse(raw)
  } catch {
    return
  }
  subscribers.forEach((handler) => {
    try {
      handler(msg)
    } catch (e) {
      console.error('[WS] handler error', e)
    }
  })
}

function scheduleReconnect() {
  clearTimeout(reconnectTimer)
  reconnectTimer = setTimeout(() => {
    connect()
    reconnectDelay = Math.min(reconnectDelay * 2, 30000)
  }, reconnectDelay)
}

function handleSessionInvalid() {
  manualClose = true
  clearTimeout(reconnectTimer)
  clearTokens()
  if (window.location.pathname !== '/login') {
    window.location.href = '/login'
  }
}

async function connect() {
  manualClose = false
  if (ws && (ws.readyState === WebSocket.CONNECTING || ws.readyState === WebSocket.OPEN)) return
  if (!localStorage.getItem('rustcam_access_token')) return

  let ticket
  try {
    ticket = await fetchTicket()
  } catch (e) {
    console.warn('[WS] ticket fetch failed, will retry', e?.message || e)
    scheduleReconnect()
    return
  }

  if (manualClose) return
  if (ws && (ws.readyState === WebSocket.CONNECTING || ws.readyState === WebSocket.OPEN)) return

  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  ws = new WebSocket(`${protocol}//${window.location.host}/ws`, [ticket])

  ws.onopen = () => {
    wsConnected.value = true
    reconnectDelay = 1000
    console.log('[WS] connected')
  }

  ws.onmessage = (event) => notify(event.data)

  ws.onclose = (event) => {
    wsConnected.value = false
    ws = null
    if (event.code === 4401) {
      console.warn('[WS] session revoked')
      handleSessionInvalid()
      return
    }
    if (!manualClose) {
      console.log('[WS] disconnected, reconnecting...')
      scheduleReconnect()
    }
  }

  ws.onerror = () => {
    wsConnected.value = false
  }
}

function disconnect() {
  manualClose = true
  clearTimeout(reconnectTimer)
  if (ws) {
    ws.close()
    ws = null
  }
  wsConnected.value = false
}

function onMessage(handler) {
  subscribers.add(handler)
  return () => subscribers.delete(handler)
}

export function useWsConnection() {
  return { wsConnected, connect, disconnect, onMessage }
}
