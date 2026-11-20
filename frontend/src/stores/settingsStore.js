import { defineStore } from 'pinia'
import { getMapSettings, updateMapSettings } from '../api/settings'

// Map settings live in the backend database:
export const useSettingsStore = defineStore('settings', {
  state: () => ({
    browserLayers: [],
    clientLayers: [],
    mapWfsUrl: '',
    mapWfsSrs: 'EPSG:4326',
    mapAutoLocate: localStorage.getItem('mapAutoLocate') === 'true',
    settingsLoaded: false,
  }),

  actions: {
    applyMapSettings(data) {
      this.browserLayers = Array.isArray(data.browser_layers) ? data.browser_layers : []
      this.clientLayers = Array.isArray(data.client_layers) ? data.client_layers : []
      const wfs = data.wfs || {}
      this.mapWfsUrl = wfs.url || ''
      this.mapWfsSrs = wfs.srs || 'EPSG:4326'
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
        browser_layers: this.browserLayers,
        client_layers: this.clientLayers,
        wfs: { url: this.mapWfsUrl, srs: this.mapWfsSrs },
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
