import { request } from '../utils/request'

export async function getMapSettings() {
  const res = await request.get('/settings/map')
  return res.data.data || {
    browser_layers: [],
    client_layers: [],
    wfs: { url: '', srs: 'EPSG:4326' },
  }
}

export async function updateMapSettings(data) {
  const res = await request.put('/settings/map', data)
  return res.data.data
}
