import { defineStore } from 'pinia'
import * as api from '../api/groups'
import { useDeviceTreeStore } from './deviceTreeStore'

async function refreshDeviceTree() {
  try {
    await useDeviceTreeStore().refresh()
  } catch (e) {
    console.error('[groupStore] 刷新设备树失败:', e)
  }
}

export const useGroupStore = defineStore('groups', {
  state: () => ({
    groupTree: [],
    loading: false,
  }),

  actions: {
    async fetchGroupTree() {
      this.loading = true
      try {
        this.groupTree = await api.getGroupTree()
      } finally {
        this.loading = false
      }
    },

    async createGroup(data) {
      const group = await api.createGroup(data)
      await this.fetchGroupTree()
      await refreshDeviceTree()
      return group
    },

    async updateGroup(id, data) {
      await api.updateGroup(id, data)
      await this.fetchGroupTree()
      await refreshDeviceTree()
    },

    async deleteGroup(id) {
      await api.deleteGroup(id)
      await this.fetchGroupTree()
      await refreshDeviceTree()
    },

    async assignDeviceGroup(deviceId, groupId) {
      const result = await api.assignDeviceGroup(deviceId, groupId)
      await refreshDeviceTree()
      return result
    },
  },
})
