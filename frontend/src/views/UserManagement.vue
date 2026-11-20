<template>
  <PageView title="用户管理">
    <template #toolbar>
      <el-button type="primary" :icon="Plus" @click="openCreateModal">创建用户</el-button>
    </template>

    <DataCard>
      <el-skeleton animated :loading="loading" :rows="4">
        <template #default>
          <el-table :data="users" border stripe size="small" style="margin-top: 0">
            <el-table-column label="用户名" prop="username" />
            <el-table-column label="角色" min-width="160">
              <template #default="{ row }">
                <el-tag v-for="role in row.roles" :key="role" size="small" style="margin-right: 4px">{{ role }}</el-tag>
                <span v-if="!row.roles?.length">-</span>
              </template>
            </el-table-column>
            <el-table-column label="超级用户" width="120" align="center">
              <template #default="{ row }">
                <el-switch
                  :model-value="!!row.is_super"
                  :disabled="!canManageSuper || row.id === currentUserId"
                  @change="(value) => toggleSuper(row, value)"
                />
              </template>
            </el-table-column>
            <el-table-column label="创建时间" width="180">
              <template #default="{ row }">{{ formatDate(row.created_at) }}</template>
            </el-table-column>
            <el-table-column label="操作" width="220">
              <template #default="{ row }">
                <el-button size="small" @click="openEditModal(row)">编辑</el-button>
                <el-button size="small" @click="openRolesModal(row)">角色</el-button>
                <el-button v-if="row.id !== currentUserId" size="small" type="danger" plain @click="confirmDelete(row)">删除</el-button>
              </template>
            </el-table-column>
          </el-table>
          <EmptyState v-if="users.length === 0" title="暂无用户" />
        </template>
      </el-skeleton>
    </DataCard>

    <template #footer>
      <PaginationBar
        v-model:page="currentPage"
        v-model:page-size="pageSize"
        :total="total"
        show-sizes
        @change="fetchUsers"
      />
    </template>

    <AppDialog v-model="showCreateModal" title="创建用户" size="sm" :close-on-click-modal="false">
      <el-form label-position="top">
        <el-form-item label="用户名 *" required>
          <el-input v-model="createForm.username" />
        </el-form-item>
        <el-form-item label="密码 *" required>
          <el-input v-model="createForm.password" type="password" show-password />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="showCreateModal = false">取消</el-button>
        <el-button type="primary" :loading="submitting" @click="submitCreate">创建</el-button>
      </template>
    </AppDialog>

    <AppDialog v-model="showEditModal" :title="`编辑用户: ${editUser?.username}`" size="sm" :close-on-click-modal="false">
      <el-form-item label="新密码 (留空保持不变)">
        <el-input v-model="editForm.password" type="password" show-password placeholder="留空保持不变" />
      </el-form-item>
      <template #footer>
        <el-button @click="showEditModal = false">取消</el-button>
        <el-button type="primary" :loading="submitting" @click="submitEdit">保存</el-button>
      </template>
    </AppDialog>

    <AppDialog v-model="showRolesModal" :title="`分配角色: ${rolesUser?.username}`" size="sm" :close-on-click-modal="false">
      <div style="display: flex; flex-direction: column; gap: 8px">
        <div
          v-for="role in availableRoles"
          :key="role.id"
          style="display: flex; align-items: center; gap: 8px; padding: 8px; border: 1px solid var(--border); border-radius: 4px; cursor: pointer"
          @click="toggleRole(role.id)"
        >
          <el-checkbox :model-value="rolesForm.roleIds.includes(role.id)" />
          <div>
            <div style="font-weight: 500">{{ role.name }}</div>
            <div style="font-size: 12px; color: var(--text-muted)">{{ role.description || role.name }}</div>
          </div>
        </div>
      </div>
      <template #footer>
        <el-button @click="showRolesModal = false">取消</el-button>
        <el-button type="primary" :loading="submitting" @click="submitRoles">保存</el-button>
      </template>
    </AppDialog>
  </PageView>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { ElMessageBox } from 'element-plus'
import { Plus } from '@element-plus/icons-vue'
import { useAuthStore } from '../stores/authStore'
import { useToast } from '../composables/useToast'
import * as userApi from '../api/users'
import * as roleApi from '../api/roles'
import DataCard from '../components/common/DataCard.vue'
import PageView from '../components/common/PageView.vue'
import EmptyState from '../components/common/EmptyState.vue'
import AppDialog from '../components/common/AppDialog.vue'
import PaginationBar from '../components/common/PaginationBar.vue'

const auth = useAuthStore()
const toast = useToast()

const users = ref([])
const availableRoles = ref([])
const loading = ref(false)
const submitting = ref(false)
const total = ref(0)
const currentPage = ref(1)
const pageSize = ref(20)
const showCreateModal = ref(false)
const showEditModal = ref(false)
const showRolesModal = ref(false)
const editUser = ref(null)
const rolesUser = ref(null)
const currentUserId = auth.currentUser?.id
const canManageSuper = computed(() => !!auth.currentUser?.is_super)

const createForm = ref({ username: '', password: '' })
const editForm = ref({ password: '' })
const rolesForm = ref({ roleIds: [] })

onMounted(async () => { await Promise.all([fetchUsers(), fetchRoles()]) })

async function fetchUsers() {
  loading.value = true
  try {
      const res = await userApi.getUsers({
        limit: pageSize.value,
        offset: (currentPage.value - 1) * pageSize.value,
      })
      total.value = res.total || 0
      users.value = res.items || []
      const maxPage = Math.max(1, Math.ceil(total.value / pageSize.value))
    if (currentPage.value > maxPage) {
      currentPage.value = maxPage
      await fetchUsers()
    }
  } catch (e) {
    toast.error('获取用户列表失败: ' + e.message)
  } finally {
    loading.value = false
  }
}
async function fetchRoles() { try { availableRoles.value = await roleApi.getRoles() } catch (e) { toast.error('获取角色列表失败') } }

function openCreateModal() { createForm.value = { username: '', password: '' }; showCreateModal.value = true }
async function submitCreate() { submitting.value = true; try { await userApi.createUser(createForm.value); toast.success('用户创建成功'); showCreateModal.value = false; await fetchUsers() } catch {} finally { submitting.value = false } }

function openEditModal(user) { editUser.value = user; editForm.value = { password: '' }; showEditModal.value = true }
async function submitEdit() { if (!editForm.value.password) { showEditModal.value = false; return }; submitting.value = true; try { await userApi.updateUser(editUser.value.id, { password: editForm.value.password }); toast.success('用户更新成功'); showEditModal.value = false } catch {} finally { submitting.value = false } }

function openRolesModal(user) { rolesUser.value = user; rolesForm.value = { roleIds: [...(user.role_ids || [])] }; showRolesModal.value = true }
function toggleRole(id) { const idx = rolesForm.value.roleIds.indexOf(id); if (idx >= 0) rolesForm.value.roleIds.splice(idx, 1); else rolesForm.value.roleIds.push(id) }
async function submitRoles() { submitting.value = true; try { await userApi.assignUserRoles(rolesUser.value.id, rolesForm.value.roleIds); toast.success('角色分配成功'); showRolesModal.value = false; await fetchUsers() } catch {} finally { submitting.value = false } }

async function confirmDelete(user) { try { await ElMessageBox.confirm(`确定删除用户 "${user.username}" 吗？`, '确认删除', { type: 'warning' }); await userApi.deleteUser(user.id); toast.success('用户已删除'); await fetchUsers() } catch {} }

async function toggleSuper(user, value) {
  const tip = value
    ? `确定把 "${user.username}" 设为超级用户吗？超级用户自动拥有全部权限。`
    : `确定取消 "${user.username}" 的超级用户标志吗？该用户会被强制重新登录。`
  try {
    await ElMessageBox.confirm(tip, '确认修改', { type: 'warning' })
  } catch {
    return
  }
  try {
    await userApi.setUserSuper(user.id, value)
    toast.success(value ? '已设为超级用户' : '已取消超级用户标志')
    await fetchUsers()
  } catch (e) {
    toast.error('修改失败: ' + (e.message || e))
    await fetchUsers()
  }
}

function formatDate(dateStr) { if (!dateStr) return '-'; return new Date(dateStr).toLocaleString('zh-CN', { hour12: false }) }
</script>
