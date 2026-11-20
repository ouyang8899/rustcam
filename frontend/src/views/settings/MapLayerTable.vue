<template>
  <div>
    <el-table :data="layers" border stripe size="small" empty-text="暂无图层">
      <el-table-column label="名称" min-width="140">
        <template #default="{ row }">
          <div class="layer-name">{{ row.name || '未命名图层' }}</div>
          <div class="layer-url" :title="row.url">{{ row.url || '-' }}</div>
        </template>
      </el-table-column>
      <el-table-column label="维度" width="64" align="center">
        <template #default="{ row }">
          <el-tag size="small" :type="row.dim === '3d' ? 'warning' : 'info'">
            {{ row.dim === '3d' ? '3D' : '2D' }}
          </el-tag>
        </template>
      </el-table-column>
      <el-table-column label="协议" width="88" align="center">
        <template #default="{ row }">{{ protocolLabel(row.protocol) }}</template>
      </el-table-column>
      <el-table-column label="层级" width="84" align="center">
        <template #default="{ row }">{{ row.min_zoom }} - {{ row.max_zoom }}</template>
      </el-table-column>
      <el-table-column label="状态" width="72" align="center">
        <template #default="{ row }">
          <el-tag size="small" :type="row.enabled === false ? 'info' : 'success'">
            {{ row.enabled === false ? '停用' : '启用' }}
          </el-tag>
        </template>
      </el-table-column>
      <el-table-column label="操作" width="120" align="center">
        <template #default="{ $index }">
          <el-button link type="primary" @click="$emit('edit', $index)">编辑</el-button>
          <el-button link type="danger" @click="$emit('remove', $index)">删除</el-button>
        </template>
      </el-table-column>
    </el-table>

    <el-button size="small" :icon="Plus" style="margin-top: 8px" @click="$emit('add')">
      添加图层
    </el-button>
  </div>
</template>

<script setup>
import { Plus } from '@element-plus/icons-vue'
import { protocolInfo } from '../../utils/mapTiles'

defineProps({
  layers: { type: Array, default: () => [] },
})

defineEmits(['add', 'edit', 'remove'])

function protocolLabel(protocol) {
  const value = String(protocol || '').trim()
  if (!value) return '自动'
  return (protocolInfo(value).label || '').split('（')[0]
}
</script>

<style scoped>
:deep(.el-table .cell) {
  white-space: nowrap;
}

:deep(.el-table__body td:first-child .cell) {
  white-space: normal;
}

.layer-name {
  font-weight: var(--weight-medium);
  color: var(--text-primary);
}

.layer-url {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
