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
    /** 当前列表视图（受 includeDisabled 过滤）。 */
    adapters: [],
    /** 全量 adapter（含停用），用于统计与下拉选项。 */
    allAdapters: [],
    loading: false,
    /** 是否把已停用（软删除）的 adapter 也列出来。 */
    includeDisabled: false,
  }),

  getters: {
    /** 启用中的 adapter：可绑定新无人机。 */
    enabledAdapters: (state) => state.allAdapters.filter(a => a.enabled),
    /** 下拉选项：停用的仍可见但标注出来，避免误绑。 */
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
        // 一次取全量：统计口径不随“显示已停用”开关变化。
        const list = await api.getAdapters(true)
        this.allAdapters = Array.isArray(list) ? list : []
        this.adapters = includeDisabled
          ? this.allAdapters
          : this.allAdapters.filter(a => a.enabled)
      } finally {
        this.loading = false
      }
    },

    /** `editing=true` 时按更新处理；新增冲突交给页面引导恢复。 */
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
