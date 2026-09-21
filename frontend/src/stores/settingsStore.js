import { defineStore } from 'pinia'

export const useSettingsStore = defineStore('settings', {
  state: () => ({
    playerType: localStorage.getItem('playerType') || 'rtsp',
    mapWfsUrl: localStorage.getItem('mapWfsUrl') || '',
    mapTmsUrl: localStorage.getItem('mapTmsUrl') || '',
    mapAutoLocate: localStorage.getItem('mapAutoLocate') === 'true',
  }),

  actions: {
    setPlayerType(type) {
      this.playerType = type
      localStorage.setItem('playerType', type)
    },
    setMapWfsUrl(url) {
      this.mapWfsUrl = url
      localStorage.setItem('mapWfsUrl', url)
    },
    setMapTmsUrl(url) {
      this.mapTmsUrl = url
      localStorage.setItem('mapTmsUrl', url)
    },
    setMapAutoLocate(enabled) {
      this.mapAutoLocate = enabled
      localStorage.setItem('mapAutoLocate', String(enabled))
    },
  },
})
