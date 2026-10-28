import { defineStore } from 'pinia'
import { getMapSettings, updateMapSettings } from '../api/settings'

// Map settings live in the backend database:
//   browser_tms_url - TMS tile url used by the web map
//   client_tms_url  - TMS tile url used by the desktop client map
//   wfs_url         - WFS service url shared by web and client
//   *_min/max_zoom  - zoom range for the web map and the client map
export const useSettingsStore = defineStore('settings', {
  state: () => ({
    browserTmsUrl: '',
    clientTmsUrl: '',
    mapWfsUrl: '',
    browserMinZoom: 0,
    browserMaxZoom: 19,
    clientMinZoom: 0,
    clientMaxZoom: 19,
    mapAutoLocate: localStorage.getItem('mapAutoLocate') === 'true',
    settingsLoaded: false,
  }),

  actions: {
    applyMapSettings(data) {
      this.browserTmsUrl = data.browser_tms_url || ''
      this.clientTmsUrl = data.client_tms_url || ''
      this.mapWfsUrl = data.wfs_url || ''
      this.browserMinZoom = Number.isInteger(data.browser_min_zoom) ? data.browser_min_zoom : 0
      this.browserMaxZoom = Number.isInteger(data.browser_max_zoom) ? data.browser_max_zoom : 19
      this.clientMinZoom = Number.isInteger(data.client_min_zoom) ? data.client_min_zoom : 0
      this.clientMaxZoom = Number.isInteger(data.client_max_zoom) ? data.client_max_zoom : 19
    },

    async loadMapSettings(force = false) {
      if (this.settingsLoaded && !force) return
      try {
        this.applyMapSettings(await getMapSettings())
        this.settingsLoaded = true
      } catch (e) {
        console.error('[settings] Failed to load map settings:', e)
      }
    },

    async saveMapSettings() {
      const data = await updateMapSettings({
        browser_tms_url: this.browserTmsUrl,
        client_tms_url: this.clientTmsUrl,
        wfs_url: this.mapWfsUrl,
        browser_min_zoom: this.browserMinZoom,
        browser_max_zoom: this.browserMaxZoom,
        client_min_zoom: this.clientMinZoom,
        client_max_zoom: this.clientMaxZoom,
      })
      if (data) this.applyMapSettings(data)
      this.settingsLoaded = true
      return data
    },

    setMapAutoLocate(enabled) {
      this.mapAutoLocate = enabled
      localStorage.setItem('mapAutoLocate', String(enabled))
    },
  },
})
