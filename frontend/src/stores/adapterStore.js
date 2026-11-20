// @ts-check
import { defineStore } from 'pinia'
import { ElMessage } from 'element-plus'
import * as api from '../api/adapter'

export const useAdapterStore = defineStore('adapter', {
  state: () => ({
    adapters: [],
    allAdapters: [],
    loading: false,
    includeDisabled: false,
  }),

  getters: {
    enabledAdapters: (state) => state.allAdapters.filter(a => a.enabled),
    adapterOptions: (state) =>
      state.allAdapters.map(a => ({
        value: a.adapter_id,
        label: a.name ? `${a.name}（${a.adapter_id}）` : a.adapter_id,
        disabled: !a.enabled,
      })),
    onlineCount: (state) => state.allAdapters.filter(a => a.online).length,
    totalCount: (state) => state.allAdapters.length,
    disabledCount: (state) => state.allAdapters.filter(a => !a.enabled).length,
    boundTotal: (state) =>
      state.allAdapters.reduce((sum, a) => sum + (a.bound_drones || 0), 0),
  },

  actions: {
    async fetchAdapters(includeDisabled = this.includeDisabled) {
      this.loading = true
      try {
        const list = await api.getAdapters(true)
        this.allAdapters = Array.isArray(list) ? list : []
        this.adapters = includeDisabled
          ? this.allAdapters
          : this.allAdapters.filter(a => a.enabled)
      } finally {
        this.loading = false
      }
    },

    async saveAdapter(adapter, editing = false) {
      const saved = editing
        ? await api.updateAdapter(adapter.adapter_id, adapter)
        : await api.createAdapter(adapter)
      await this.fetchAdapters()
      ElMessage.success('Adapter 配置已保存')
      return saved
    },

    async disableAdapter(adapterId) {
      await api.disableAdapter(adapterId)
      await this.fetchAdapters()
      ElMessage.success('Adapter 已停用')
    },

    async restoreAdapter(adapterId) {
      await api.restoreAdapter(adapterId)
      await this.fetchAdapters()
      ElMessage.success('Adapter 已恢复')
    },
  },
})
