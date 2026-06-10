import { defineStore } from 'pinia'
import { ElMessage } from 'element-plus'
import * as api from '../api/devices'

export const useDeviceStore = defineStore('devices', {
  state: () => ({
    devices: [],
    loading: false,
    error: null,
    total: 0,
    limit: 50,
    offset: 0,
  }),

  actions: {
    async fetchDevices(limit = 50, offset = 0) {
      this.loading = true
      this.error = null
      try {
        const data = await api.getDevices({ limit, offset })
        this.devices = data.items
        this.total = data.total
        this.limit = data.limit
        this.offset = data.offset
      } catch (e) {
        this.error = e.message
        throw e
      } finally {
        this.loading = false
      }
    },

    async createDevice(data) {
      const device = await api.createDevice(data)
      this.devices.unshift(device)
      this.total++
      ElMessage.success('设备创建成功')
      return device
    },

    async updateDevice(id, data) {
      const device = await api.updateDevice(id, data)
      const index = this.devices.findIndex(d => d.id === id)
      if (index !== -1) this.devices[index] = device
      ElMessage.success('设备更新成功')
      return device
    },

    async deleteDevice(id) {
      await api.deleteDevice(id)
      this.devices = this.devices.filter(d => d.id !== id)
      this.total--
      ElMessage.success('设备删除成功')
    },

    async playDevice(id) {
      const data = await api.playDevice(id)
      ElMessage.success('播放已启动')
      return data
    },

    async stopDevice(id) {
      await api.stopDevice(id)
      ElMessage.success('播放已停止')
      return true
    },

    async fetchDevice(id) {
      this.loading = true
      try {
        return await api.getDevice(id)
      } finally {
        this.loading = false
      }
    },

    async getPlayLinks(id) {
      return await api.getPlayLinks(id)
    },

    async fetchPtzPresets(id) {
      return await api.getPtzPresets(id)
    },

    async createPtzPreset(id, name) {
      const preset = await api.createPtzPreset(id, name)
      ElMessage.success('预制位已创建')
      return preset
    },

    async deletePtzPreset(id, token) {
      await api.deletePtzPreset(id, token)
      ElMessage.success('预制位已删除')
    },

    async getPtzStatus(id) {
      return await api.getPtzStatus(id)
    },

    async ptzControl(id, command, speed = 50, presetToken = null) {
      await api.ptzControl(id, command, speed, presetToken)
      if (command === 'stop' || presetToken) {
        ElMessage.success(command === 'stop' ? '已停止' : '预制位已调用')
      }
    },
  },
})
