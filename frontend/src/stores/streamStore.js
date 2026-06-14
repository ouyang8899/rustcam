import { defineStore } from 'pinia'
import { ElMessage } from 'element-plus'
import * as api from '../api/streams'

export const useStreamStore = defineStore('streams', {
  state: () => ({
    streams: [],
    loading: false,
    error: null,
    total: 0,
    page: 1,
    pageSize: 12,
  }),

  actions: {
    async fetchStreams(offset = 0) {
      this.loading = true
      this.error = null
      try {
        const data = await api.getStreams({ limit: this.pageSize, offset })
        this.streams = data.items || data
        this.total = data.total || this.streams.length
      } catch (e) {
        this.error = e.message
        throw e
      } finally {
        this.loading = false
      }
    },

    async getPlayUrl(id, protocol = 'hls') {
      return await api.getStreamPlayUrl(id, protocol)
    },

    async startStream(deviceId, rtspUrl) {
      this.loading = true
      try {
        const body = { device_id: deviceId }
        if (rtspUrl !== undefined && rtspUrl !== null && rtspUrl !== '') {
          body.rtsp_url = rtspUrl
        }
        const data = await api.startStream(body)
        await this.fetchStreams()
        ElMessage.success('流已启动')
        return data
      } finally {
        this.loading = false
      }
    },

    async stopStream(id) {
      await api.stopStream(id)
      await this.fetchStreams()
      ElMessage.success('流已停止')
    },

    hasActiveStream(deviceId) {
      return this.streams.some(s => s.device_id === deviceId && (s.state === 'Active' || s.state === 'Starting' || s.state === 'Recovering'))
    },
  },
})
