import { defineStore } from 'pinia'
import * as api from '../api/deviceTree'

function indexNodes(nodes, deviceMap, channelMap) {
  for (const node of nodes || []) {
    if (node.type === 'device' && node.device_tag) {
      deviceMap.set(node.device_tag, node)
    } else if (node.type === 'channel' && node.device_tag && node.channel_tag) {
      channelMap.set(`${node.device_tag}/${node.channel_tag}`, node)
    }
    indexNodes(node.children, deviceMap, channelMap)
  }
}

function flattenGroups(nodes, depth = 0, out = []) {
  for (const node of nodes || []) {
    if (node.type !== 'group') continue
    out.push({
      id: node.id,
      name: node.name,
      parent_id: node.parent_id ?? null,
      depth,
      label: `${'　'.repeat(depth)}${depth > 0 ? '└ ' : ''}${node.name}`,
    })
    flattenGroups(node.children, depth + 1, out)
  }
  return out
}

export const useDeviceTreeStore = defineStore('deviceTree', {
  state: () => ({
    nodes: [],
    regionNodes: [],
    stats: {},
    loading: false,
    refreshing: false,
    loaded: false,
    revision: 0,
    error: null,
    lastLoadedAt: null,
  }),

  getters: {
    deviceIndex: (state) => {
      const map = new Map()
      indexNodes(state.nodes, map, new Map())
      return map
    },
    channelIndex: (state) => {
      const map = new Map()
      indexNodes(state.nodes, new Map(), map)
      return map
    },
    flatGroups: (state) => flattenGroups(state.nodes),
  },

  actions: {
    async fetchTree(force = false) {
      if (this.loaded && !force) return
      if (this.loading) return
      this.loading = true
      this.error = null
      try {
        const data = await api.getDeviceTree()
        this.nodes = data.nodes || []
        this.regionNodes = data.region_nodes || []
        this.stats = data.stats || {}
        this.loaded = true
        this.revision += 1
        this.lastLoadedAt = Date.now()
      } catch (e) {
        this.error = e?.message || String(e)
        throw e
      } finally {
        this.loading = false
      }
    },

    async refresh() {
      if (this.refreshing) return
      this.refreshing = true
      this.error = null
      try {
        const data = await api.getDeviceTree()
        this.nodes = data.nodes || []
        this.regionNodes = data.region_nodes || []
        this.stats = data.stats || {}
        this.loaded = true
        this.revision += 1
        this.lastLoadedAt = Date.now()
      } catch (e) {
        this.error = e?.message || String(e)
        throw e
      } finally {
        this.refreshing = false
      }
    },
  },
})
