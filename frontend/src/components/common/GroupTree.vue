<template>
  <div class="group-tree" :class="{ 'group-tree--fill': fill }" :style="fill ? null : { height }">
    <div v-if="searchable || refreshable || $slots.toolbar" class="group-tree__bar">
      <slot name="search">
        <el-input
          v-if="searchable"
          v-model="keyword"
          class="group-tree__search"
          size="small"
          clearable
          :placeholder="searchPlaceholder"
          @input="onSearchInput"
          @clear="onSearchInput"
        >
          <template #prefix><el-icon><Search /></el-icon></template>
        </el-input>
      </slot>
      <div class="group-tree__actions">
        <slot name="toolbar" />
        <el-tooltip v-if="refreshable" content="刷新分组树" :show-after="400">
          <el-button
            size="small"
            circle
            text
            :icon="Refresh"
            :loading="treeStore.refreshing"
            @click="onRefresh"
          />
        </el-tooltip>
      </div>
    </div>

    <div class="group-tree__body">
      <el-skeleton v-if="showSkeleton" animated :rows="4" />
      <template v-else>
        <el-tree
          ref="treeRef"
          :data="displayNodes"
          :props="{ label: 'name', children: 'children' }"
          node-key="key"
          :expand-on-click-node="false"
          :default-expand-all="defaultExpandAll"
          :highlight-current="highlightCurrent"
          @node-click="onNodeClick"
          @node-dblclick="onNodeDblclick"
          @node-contextmenu="onNodeContextmenu"
        >
          <template #default="{ data }">
            <div
              class="group-tree__row"
              :draggable="isDraggableNode(data)"
              @dragstart="onNodeDragStart(data, $event)"
              @dragend="onNodeDragEnd(data, $event)"
            >
              <slot name="node" :node="data" :count="data.count">
                <span class="group-tree__node">
                  <span class="group-tree__node-label">{{ data.name }}</span>
                  <span v-if="data.count != null" class="group-tree__node-count">{{ data.count }}</span>
                </span>
              </slot>
            </div>
          </template>
        </el-tree>
        <EmptyState
          v-if="displayNodes.length === 0"
          :title="emptyLabel"
        />
      </template>
    </div>
  </div>
</template>

<script setup>
import { computed, onMounted, ref, watch } from 'vue'
import { Refresh, Search } from '@element-plus/icons-vue'
import { useDeviceTreeStore } from '../../stores/deviceTreeStore'
import { getDeviceTree } from '../../api/deviceTree'
import { buildDragPayload } from '../../utils/dragPayload'
import EmptyState from './EmptyState.vue'
import { useToast } from '../../composables/useToast'

const toast = useToast()

const props = defineProps({
  searchable: { type: Boolean, default: true },
  searchPlaceholder: { type: String, default: '搜索设备 / 通道名称或 TAG' },
  mode: { type: String, default: 'group' },
  showDevices: { type: Boolean, default: true },
  draggable: { type: Boolean, default: true },
  refreshable: { type: Boolean, default: true },
  defaultExpandAll: { type: Boolean, default: false },
  loading: { type: Boolean, default: false },
  height: { type: String, default: '100%' },
  fill: { type: Boolean, default: false },
  emptyText: { type: String, default: '' },
  highlightCurrent: { type: Boolean, default: true },
})

const emit = defineEmits([
  'node-click',
  'node-dblclick',
  'node-contextmenu',
  'drag-start',
  'drag-end',
  'search-change',
  'group-create',
  'group-update',
  'group-delete',
])

const treeStore = useDeviceTreeStore()
const treeRef = ref(null)

const keyword = ref('')
const searchResult = ref(null)
let searchTimer = null

const treeNodes = computed(() =>
  props.mode === 'region' ? treeStore.regionNodes : treeStore.nodes
)

function toGroupOnlyTree(nodes) {
  const strip = (list) =>
    list
      .filter((n) => n.type !== 'device' && n.type !== 'channel')
      .map((n) => ({ ...n, children: strip(n.children || []) }))
  return strip(nodes)
}

const displayNodes = computed(() => {
  const source = searchResult.value ?? treeNodes.value
  return props.showDevices ? source : toGroupOnlyTree(source)
})

const emptyLabel = computed(() =>
  props.emptyText || (props.mode === 'region' ? '暂无地区数据' : '暂无分组数据')
)

const showSkeleton = computed(() => props.loading || treeStore.loading)

onMounted(async () => {
  try {
    await treeStore.fetchTree()
  } catch (e) {
    toast.error('加载分组树失败: ' + (e?.message || e))
  }
})

watch(
  () => treeStore.revision,
  () => {
    if (keyword.value.trim()) {
      runSearch()
    }
  }
)

function onSearchInput() {
  if (searchTimer) clearTimeout(searchTimer)
  searchTimer = setTimeout(() => runSearch(), 300)
}

async function runSearch() {
  const kw = keyword.value.trim()
  if (!kw) {
    searchResult.value = null
    emit('search-change', '', displayNodes.value)
    return
  }
  try {
    const data = await getDeviceTree({ search: kw })
    searchResult.value =
      props.mode === 'region' ? data.region_nodes || [] : data.nodes || []
    emit('search-change', kw, searchResult.value)
  } catch (e) {
    searchResult.value = null
    toast.error('搜索失败: ' + (e?.message || e))
  }
}

async function onRefresh() {
  try {
    await treeStore.refresh()
    toast.success('分组树已刷新')
  } catch (e) {
    toast.error('刷新失败: ' + (e?.message || e))
  }
}

function onNodeClick(data, event) {
  emit('node-click', data, event)
}

function onNodeDblclick(data, event) {
  emit('node-dblclick', data, event)
}

function onNodeContextmenu(data, node, component, event) {
  emit('node-contextmenu', data, event)
}

function isDraggableNode(data) {
  if (!props.draggable) return false
  return data.type === 'device' || data.type === 'channel'
}

function onNodeDragStart(data, event) {
  if (!isDraggableNode(data)) return
  const payload = buildDragPayload(data)
  if (!payload) return
  event.dataTransfer.effectAllowed = 'copy'
  event.dataTransfer.setData('application/x-rustcam-node', JSON.stringify(payload))
  event.dataTransfer.setData('text/plain', JSON.stringify(payload))
  emit('drag-start', payload, event)
}

function onNodeDragEnd(data, event) {
  emit('drag-end', buildDragPayload(data), event)
}

function expandToKey(key) {
  if (!treeRef.value || !key) return
  const node = treeRef.value.getNode(key)
  if (!node) return
  let parent = node.parent
  while (parent) {
    parent.expanded = true
    parent = parent.parent
  }
  treeRef.value.setCurrentKey(key)
}

defineExpose({ expandToKey, treeRef })
</script>

<style scoped>
.group-tree__body :deep(.empty-state) {
  padding: var(--space-6) var(--space-3);
}
.group-tree {
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.group-tree--fill {
  flex: 1 1 auto;
  min-height: 0;
}

.group-tree__bar {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding-bottom: var(--space-2);
}

.group-tree__search {
  flex: 1;
  min-width: 0;
}

.group-tree__actions {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  flex-shrink: 0;
}

.group-tree__body {
  flex: 1;
  min-height: 0;
  overflow: auto;
}

.group-tree__row {
  display: flex;
  align-items: center;
  width: 100%;
  min-width: 0;
}

.group-tree__row[draggable='true'] {
  cursor: grab;
}

.group-tree__node {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  width: 100%;
  min-width: 0;
}

.group-tree__node-label {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.group-tree__node-count {
  color: var(--text-muted);
  font-family: var(--font-mono);
  font-size: var(--text-xs);
}
</style>
