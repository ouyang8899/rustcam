<template>
  <div class="page-container">
    <div class="page-header">
      <h1 class="page-title">角色管理</h1>
      <div class="page-toolbar">
        <el-button type="primary" :icon="Plus" @click="openCreateModal">创建角色</el-button>
      </div>
    </div>
    <div class="page-body">
      <DataCard>
        <el-skeleton animated :loading="loading" :rows="4">
          <template #default>
            <el-table :data="roles" style="margin-top: 0">
              <el-table-column label="角色名" prop="name" min-width="140" />
              <el-table-column label="描述" min-width="200">
                <template #default="{ row }">{{ row.description || '-' }}</template>
              </el-table-column>
              <el-table-column label="直接权限" min-width="220">
                <template #default="{ row }">
                  <el-tag
                    v-for="perm in (details[row.id]?.permissions || []).slice(0, 4)"
                    :key="perm"
                    size="small"
                    style="margin-right: 4px"
                  >{{ perm }}</el-tag>
                  <span v-if="(details[row.id]?.permissions || []).length > 4">
                    +{{ details[row.id].permissions.length - 4 }}
                  </span>
                  <span v-if="!(details[row.id]?.permissions || []).length">-</span>
                </template>
              </el-table-column>
              <el-table-column label="继承自" min-width="160">
                <template #default="{ row }">
                  <el-tag
                    v-for="parent in (details[row.id]?.parent_roles || [])"
                    :key="parent.id"
                    size="small"
                    type="warning"
                    style="margin-right: 4px"
                  >{{ parent.name }}</el-tag>
                  <span v-if="!(details[row.id]?.parent_roles || []).length">-</span>
                </template>
              </el-table-column>
              <el-table-column label="操作" width="260">
                <template #default="{ row }">
                  <el-button size="small" @click="openPermissionsModal(row)">权限</el-button>
                  <el-button size="small" @click="openParentsModal(row)">继承</el-button>
                  <el-button size="small" @click="openEditModal(row)">编辑</el-button>
                  <el-button
                    v-if="row.name !== 'Admin'"
                    size="small"
                    type="danger"
                    plain
                    @click="confirmDelete(row)"
                  >删除</el-button>
                </template>
              </el-table-column>
            </el-table>
            <el-empty v-if="roles.length === 0" description="暂无角色" />
          </template>
        </el-skeleton>
      </DataCard>
    </div>

    <el-dialog v-model="showCreateModal" title="创建角色" width="420px" :close-on-click-modal="false">
      <el-form label-position="top">
        <el-form-item label="角色名 *" required>
          <el-input v-model="createForm.name" placeholder="例如 Operator" />
        </el-form-item>
        <el-form-item label="描述">
          <el-input v-model="createForm.description" />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="showCreateModal = false">取消</el-button>
        <el-button type="primary" :loading="submitting" @click="submitCreate">创建</el-button>
      </template>
    </el-dialog>

    <el-dialog v-model="showEditModal" :title="`编辑角色: ${editRole?.name}`" width="420px" :close-on-click-modal="false">
      <el-form label-position="top">
        <el-form-item label="角色名 *" required>
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
    </el-dialog>

    <el-dialog v-model="showPermissionsModal" :title="`角色权限: ${permissionRole?.name}`" width="560px" :close-on-click-modal="false">
      <div v-for="group in permissionGroups" :key="group.object" style="margin-bottom: 16px">
        <div style="font-weight: 600; margin-bottom: 8px">{{ group.object }}</div>
        <el-checkbox-group v-model="permissionForm.permissionIds">
          <el-checkbox
            v-for="perm in group.items"
            :key="perm.id"
            :value="perm.id"
            :label="perm.name"
          />
        </el-checkbox-group>
      </div>
      <el-empty v-if="permissionGroups.length === 0" description="暂无权限点" />
      <template #footer>
        <el-button @click="showPermissionsModal = false">取消</el-button>
        <el-button type="primary" :loading="submitting" @click="submitPermissions">保存</el-button>
      </template>
    </el-dialog>

    <el-dialog v-model="showParentsModal" :title="`继承角色: ${parentsRole?.name}`" width="520px" :close-on-click-modal="false">
      <div style="font-size: 12px; color: var(--text-muted); margin-bottom: 12px">
        勾选父角色后，本角色将自动获得父角色的全部权限；不允许自身继承或形成环。
      </div>
      <el-checkbox-group v-model="parentsForm.parentRoleIds">
        <el-checkbox
          v-for="role in parentCandidates"
          :key="role.id"
          :value="role.id"
          :label="role.name"
        />
      </el-checkbox-group>
      <el-empty v-if="parentCandidates.length === 0" description="无可选父角色" />
      <template #footer>
        <el-button @click="showParentsModal = false">取消</el-button>
        <el-button type="primary" :loading="submitting" @click="submitParents">保存</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { ElMessageBox } from 'element-plus'
import { Plus } from '@element-plus/icons-vue'
import { useToast } from '../composables/useToast'
import * as roleApi from '../api/roles'
import * as permissionApi from '../api/permissions'
import DataCard from '../components/common/DataCard.vue'

const toast = useToast()

const roles = ref([])
const permissions = ref([])
const details = ref({})
const loading = ref(false)
const submitting = ref(false)

const showCreateModal = ref(false)
const showEditModal = ref(false)
const showPermissionsModal = ref(false)
const showParentsModal = ref(false)

const editRole = ref(null)
const permissionRole = ref(null)
const parentsRole = ref(null)

const createForm = ref({ name: '', description: '' })
const editForm = ref({ name: '', description: '' })
const permissionForm = ref({ permissionIds: [] })
const parentsForm = ref({ parentRoleIds: [] })

const permissionGroups = computed(() => {
  const groups = new Map()
  for (const perm of permissions.value) {
    const object = perm.name.split(':')[0] || '*'
    if (!groups.has(object)) groups.set(object, [])
    groups.get(object).push(perm)
  }
  return [...groups.entries()].map(([object, items]) => ({ object, items }))
})

const parentCandidates = computed(() => {
  if (!parentsRole.value) return []
  return roles.value.filter((role) => role.id !== parentsRole.value.id)
})

onMounted(async () => {
  await Promise.all([fetchRoles(), fetchPermissions()])
})

async function fetchRoles() {
  loading.value = true
  try {
    roles.value = await roleApi.getRoles()
    await loadDetails()
  } catch (e) {
    toast.error('获取角色列表失败: ' + e.message)
  } finally {
    loading.value = false
  }
}

async function loadDetails() {
  const entries = await Promise.all(
    roles.value.map(async (role) => {
      try {
        const detail = await roleApi.getRole(role.id)
        return [role.id, detail]
      } catch {
        return [role.id, null]
      }
    })
  )
  details.value = Object.fromEntries(entries.filter(([, detail]) => detail))
}

/**
 * 拉取全部权限点，供「角色权限」弹窗勾选。
 *
 * 权限点接口是分页的，这里一次取满（上限 500，与服务端一致）。
 */
async function fetchPermissions() {
  try {
    const res = await permissionApi.getPermissions({ limit: 500 })
    permissions.value = res.items || []
  } catch (e) {
    toast.error('获取权限列表失败: ' + e.message)
  }
}

function openCreateModal() {
  createForm.value = { name: '', description: '' }
  showCreateModal.value = true
}

async function submitCreate() {
  if (!createForm.value.name.trim()) {
    toast.error('角色名不能为空')
    return
  }
  submitting.value = true
  try {
    await roleApi.createRole({
      name: createForm.value.name.trim(),
      description: createForm.value.description || null,
    })
    toast.success('角色创建成功')
    showCreateModal.value = false
    await fetchRoles()
  } catch {
  } finally {
    submitting.value = false
  }
}

function openEditModal(role) {
  editRole.value = role
  editForm.value = { name: role.name, description: role.description || '' }
  showEditModal.value = true
}

async function submitEdit() {
  submitting.value = true
  try {
    await roleApi.updateRole(editRole.value.id, {
      name: editForm.value.name.trim(),
      description: editForm.value.description || null,
    })
    toast.success('角色更新成功')
    showEditModal.value = false
    await fetchRoles()
  } catch {
  } finally {
    submitting.value = false
  }
}

async function confirmDelete(role) {
  try {
    await ElMessageBox.confirm(`确定删除角色 "${role.name}" 吗？`, '确认删除', { type: 'warning' })
    await roleApi.deleteRole(role.id)
    toast.success('角色已删除')
    await fetchRoles()
  } catch {
  }
}

function openPermissionsModal(role) {
  permissionRole.value = role
  const detail = details.value[role.id]
  const granted = new Set(detail?.permissions || [])
  permissionForm.value = {
    permissionIds: permissions.value.filter((p) => granted.has(p.name)).map((p) => p.id),
  }
  showPermissionsModal.value = true
}

async function submitPermissions() {
  submitting.value = true
  try {
    await roleApi.setRolePermissions(permissionRole.value.id, permissionForm.value.permissionIds)
    toast.success('角色权限已更新')
    showPermissionsModal.value = false
    await fetchRoles()
  } catch {
  } finally {
    submitting.value = false
  }
}

function openParentsModal(role) {
  parentsRole.value = role
  const detail = details.value[role.id]
  parentsForm.value = {
    parentRoleIds: (detail?.parent_roles || []).map((parent) => parent.id),
  }
  showParentsModal.value = true
}

async function submitParents() {
  submitting.value = true
  try {
    await roleApi.setRoleParents(parentsRole.value.id, parentsForm.value.parentRoleIds)
    toast.success('角色继承已更新')
    showParentsModal.value = false
    await fetchRoles()
  } catch {
  } finally {
    submitting.value = false
  }
}
</script>

<style scoped>
</style>
