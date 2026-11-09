// @ts-check
import { defineStore } from 'pinia'
import { ElMessage } from 'element-plus'
import * as api from '../api/adapter'

/**
 * 机载链路实体（adapter）store。
 *
 * adapter 是「谁接管哪架飞控」的落点：它决定监听地址、可用端口段与签名策略，
 * 无人机只登记归属的 adapter 标识。
 */
export const useAdapterStore = defineStore('adapter', {
  state: () => ({
    adapters: [],
    loading: false,
    /** 是否把已停用（软删除）的 adapter 也列出来。 */
    includeDisabled: false,
  }),

  getters: {
    /** 启用中的 adapter：可绑定新无人机。 */
    enabledAdapters: (state) => state.adapters.filter(a => a.enabled),
    /** 下拉选项：停用的仍可见但标注出来，避免误绑。 */
    adapterOptions: (state) =>
      state.adapters.map(a => ({
        value: a.adapter_id,
        label: a.name ? `${a.name}（${a.adapter_id}）` : a.adapter_id,
        disabled: !a.enabled,
      })),
    onlineCount: (state) => state.adapters.filter(a => a.online).length,
  },

  actions: {
    async fetchAdapters(includeDisabled = this.includeDisabled) {
      this.loading = true
      try {
        const list = await api.getAdapters(includeDisabled)
        this.adapters = Array.isArray(list) ? list : []
      } finally {
        this.loading = false
      }
    },

    async saveAdapter(adapter) {
      const saved = adapter.adapter_id && this.adapters.some(a => a.adapter_id === adapter.adapter_id)
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
