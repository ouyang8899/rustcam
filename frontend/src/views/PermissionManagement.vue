<template>
  <PageView title="权限管理">
    <template #toolbar>
      <el-button type="primary" :icon="Plus" @click="openCreateModal">创建权限</el-button>
    </template>

    <DataCard>
      <el-skeleton animated :loading="loading" :rows="4">
        <template #default>
          <el-table :data="permissions" border stripe size="small" style="margin-top: 0">
            <el-table-column label="权限名" prop="name" min-width="200" />
            <el-table-column label="对象" width="140">
              <template #default="{ row }">{{ objectOf(row.name) }}</template>
            </el-table-column>
            <el-table-column label="动作" width="140">
              <template #default="{ row }">{{ actionOf(row.name) }}</template>
            </el-table-column>
            <el-table-column label="描述" min-width="180">
              <template #default="{ row }">{{ row.description || '-' }}</template>
            </el-table-column>
            <el-table-column label="创建时间" width="180">
              <template #default="{ row }">{{ formatDate(row.created_at) }}</template>
            </el-table-column>
            <el-table-column label="操作" width="160">
              <template #default="{ row }">
                <el-button size="small" @click="openEditModal(row)">编辑</el-button>
                <el-button size="small" type="danger" plain @click="confirmDelete(row)">删除</el-button>
              </template>
            </el-table-column>
          </el-table>
          <EmptyState v-if="permissions.length === 0" title="暂无权限点" />
        </template>
      </el-skeleton>
    </DataCard>

    <template #footer>
      <PaginationBar
        v-model:page="currentPage"
        v-model:page-size="pageSize"
        :total="total"
        show-sizes
        @change="fetchPermissions"
      />
    </template>

    <AppDialog v-model="showCreateModal" title="创建权限" size="sm" :close-on-click-modal="false">
      <el-form label-position="top">
        <el-form-item label="权限名 *" required>
          <el-input v-model="createForm.name" placeholder="例如 drone:control:acquire" />
          <div style="font-size: 12px; color: var(--text-muted)">格式为 对象:动作，动作可含冒号</div>
        </el-form-item>
        <el-form-item label="描述">
          <el-input v-model="createForm.description" />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="showCreateModal = false">取消</el-button>
        <el-button type="primary" :loading="submitting" @click="submitCreate">创建</el-button>
      </template>
    </AppDialog>

    <AppDialog v-model="showEditModal" :title="`编辑权限: ${editPermission?.name}`" size="sm" :close-on-click-modal="false">
      <el-form label-position="top">
        <el-form-item label="权限名 *" required>
          <el-input v-model="editForm.name" />
        </el-form-item>
        <el-form-item label="描述">
          <el-input v-model="editForm.description" />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="showEditModal = false">取消</el-button>
        <el-button type="primary" :loading="submitting" @click="submitEdit">保存</el-button>
      </template>
    </AppDialog>
  </PageView>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { ElMessageBox } from 'element-plus'
import { Plus } from '@element-plus/icons-vue'
import { useToast } from '../composables/useToast'
import * as permissionApi from '../api/permissions'
import DataCard from '../components/common/DataCard.vue'
import PageView from '../components/common/PageView.vue'
import EmptyState from '../components/common/EmptyState.vue'
import AppDialog from '../components/common/AppDialog.vue'
import PaginationBar from '../components/common/PaginationBar.vue'

const toast = useToast()

const permissions = ref([])
const loading = ref(false)
const submitting = ref(false)
const total = ref(0)
const currentPage = ref(1)
const pageSize = ref(20)
const showCreateModal = ref(false)
const showEditModal = ref(false)
const editPermission = ref(null)

const createForm = ref({ name: '', description: '' })
const editForm = ref({ name: '', description: '' })

onMounted(async () => {
  await fetchPermissions()
})

async function fetchPermissions() {
  loading.value = true
  try {
      const res = await permissionApi.getPermissions({
        limit: pageSize.value,
        offset: (currentPage.value - 1) * pageSize.value,
      })
      total.value = res.total || 0
      permissions.value = res.items || []
      const maxPage = Math.max(1, Math.ceil(total.value / pageSize.value))
    if (currentPage.value > maxPage) {
      currentPage.value = maxPage
      await fetchPermissions()
    }
  } catch (e) {
    toast.error('获取权限列表失败: ' + e.message)
  } finally {
    loading.value = false
  }
}

function objectOf(name) {
  return name.split(':')[0] || '*'
}

function actionOf(name) {
  const parts = name.split(':')
  return parts.slice(1).join(':') || '*'
}

function openCreateModal() {
  createForm.value = { name: '', description: '' }
  showCreateModal.value = true
}

async function submitCreate() {
  submitting.value = true
  try {
    await permissionApi.createPermission({
      name: createForm.value.name.trim(),
      description: createForm.value.description || null,
    })
    toast.success('权限创建成功')
    showCreateModal.value = false
    await fetchPermissions()
  } catch {
  } finally {
    submitting.value = false
  }
}

function openEditModal(permission) {
  editPermission.value = permission
  editForm.value = { name: permission.name, description: permission.description || '' }
  showEditModal.value = true
}

async function submitEdit() {
  submitting.value = true
  try {
    await permissionApi.updatePermission(editPermission.value.id, {
      name: editForm.value.name.trim(),
      description: editForm.value.description || null,
    })
    toast.success('权限更新成功')
    showEditModal.value = false
    await fetchPermissions()
  } catch {
  } finally {
    submitting.value = false
  }
}

async function confirmDelete(permission) {
  try {
    await ElMessageBox.confirm(`确定删除权限 "${permission.name}" 吗？`, '确认删除', { type: 'warning' })
    await permissionApi.deletePermission(permission.id)
    toast.success('权限已删除')
    await fetchPermissions()
  } catch {
  }
}

function formatDate(dateStr) {
  if (!dateStr) return '-'
  return new Date(dateStr).toLocaleString('zh-CN', { hour12: false })
}
</script>
