import { request } from '../utils/request'

export async function getMapSettings() {
  const res = await request.get('/settings/map')
  return res.data.data || {
    browser_tms_url: '',
    client_tms_url: '',
    wfs_url: '',
    browser_min_zoom: 0,
    browser_max_zoom: 19,
    client_min_zoom: 0,
    client_max_zoom: 19,
  }
}

export async function updateMapSettings(data) {
  const res = await request.put('/settings/map', data)
  return res.data.data
}
