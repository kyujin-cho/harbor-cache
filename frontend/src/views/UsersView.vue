<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { usersApi, type User, type CreateUserRequest } from '../api/client'
import {
  ArrowPathIcon,
  PlusIcon,
  PencilIcon,
  TrashIcon,
  XMarkIcon
} from '@heroicons/vue/24/outline'

const users = ref<User[]>([])
const loading = ref(true)
const actionLoading = ref(false)
const error = ref('')
const success = ref('')

// Modal state
const showModal = ref(false)
const modalMode = ref<'create' | 'edit'>('create')
const editingUser = ref<User | null>(null)
const showDeleteConfirm = ref(false)
const deletingUser = ref<User | null>(null)

// Form state
const form = ref({
  username: '',
  password: '',
  role: 'read-only'
})

const roles = [
  { value: 'admin', label: 'Admin' },
  { value: 'read-write', label: 'Read/Write' },
  { value: 'read-only', label: 'Read Only' }
]

async function fetchUsers() {
  loading.value = true
  error.value = ''
  try {
    const response = await usersApi.list()
    users.value = response.data
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Failed to fetch users'
  } finally {
    loading.value = false
  }
}

function openCreateModal() {
  modalMode.value = 'create'
  editingUser.value = null
  form.value = { username: '', password: '', role: 'read-only' }
  showModal.value = true
}

function openEditModal(user: User) {
  modalMode.value = 'edit'
  editingUser.value = user
  form.value = { username: user.username, password: '', role: user.role }
  showModal.value = true
}

function closeModal() {
  showModal.value = false
  editingUser.value = null
  form.value = { username: '', password: '', role: 'read-only' }
}

async function handleSubmit() {
  actionLoading.value = true
  error.value = ''
  success.value = ''

  try {
    if (modalMode.value === 'create') {
      await usersApi.create(form.value as CreateUserRequest)
      success.value = `User "${form.value.username}" created successfully`
    } else if (editingUser.value) {
      const updateData: { role?: string; password?: string } = {}
      if (form.value.role !== editingUser.value.role) {
        updateData.role = form.value.role
      }
      if (form.value.password) {
        updateData.password = form.value.password
      }
      await usersApi.update(editingUser.value.id, updateData)
      success.value = `User "${editingUser.value.username}" updated successfully`
    }
    closeModal()
    await fetchUsers()
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Operation failed'
  } finally {
    actionLoading.value = false
  }
}

function confirmDelete(user: User) {
  deletingUser.value = user
  showDeleteConfirm.value = true
}

async function handleDelete() {
  if (!deletingUser.value) return

  actionLoading.value = true
  error.value = ''
  success.value = ''

  try {
    await usersApi.delete(deletingUser.value.id)
    success.value = `User "${deletingUser.value.username}" deleted successfully`
    showDeleteConfirm.value = false
    deletingUser.value = null
    await fetchUsers()
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Delete failed'
  } finally {
    actionLoading.value = false
  }
}

function formatDate(dateStr: string) {
  return new Date(dateStr).toLocaleDateString()
}

onMounted(fetchUsers)
</script>

<template>
  <div class="p-8">
    <div class="mb-8 flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold text-gray-900">User Management</h1>
        <p class="mt-1 text-sm text-gray-500">Manage user accounts and permissions</p>
      </div>
      <div class="flex gap-2">
        <button @click="fetchUsers" :disabled="loading" class="btn btn-secondary flex items-center gap-2">
          <ArrowPathIcon class="h-4 w-4" :class="{ 'animate-spin': loading }" />
          Refresh
        </button>
        <button @click="openCreateModal" class="btn btn-primary flex items-center gap-2">
          <PlusIcon class="h-4 w-4" />
          Add User
        </button>
      </div>
    </div>

    <!-- Alerts -->
    <div v-if="error" class="mb-6 rounded-md bg-red-50 p-4">
      <p class="text-sm text-red-700">{{ error }}</p>
    </div>
    <div v-if="success" class="mb-6 rounded-md bg-green-50 p-4">
      <p class="text-sm text-green-700">{{ success }}</p>
    </div>

    <!-- Loading state -->
    <div v-if="loading" class="flex items-center justify-center h-64">
      <ArrowPathIcon class="h-8 w-8 text-gray-400 animate-spin" />
    </div>

    <!-- Users table -->
    <div v-else class="card p-0 overflow-hidden">
      <table class="min-w-full divide-y divide-gray-200">
        <thead class="bg-gray-50">
          <tr>
            <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
              Username
            </th>
            <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
              Role
            </th>
            <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
              Created
            </th>
            <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase tracking-wider">
              Actions
            </th>
          </tr>
        </thead>
        <tbody class="bg-white divide-y divide-gray-200">
          <tr v-for="user in users" :key="user.id">
            <td class="px-6 py-4 whitespace-nowrap">
              <span class="font-medium text-gray-900">{{ user.username }}</span>
            </td>
            <td class="px-6 py-4 whitespace-nowrap">
              <span
                class="px-2 py-1 text-xs font-medium rounded-full"
                :class="{
                  'bg-purple-100 text-purple-800': user.role === 'admin',
                  'bg-blue-100 text-blue-800': user.role === 'read-write',
                  'bg-gray-100 text-gray-800': user.role === 'read-only'
                }"
              >
                {{ user.role }}
              </span>
            </td>
            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-500">
              {{ formatDate(user.created_at) }}
            </td>
            <td class="px-6 py-4 whitespace-nowrap text-right text-sm font-medium">
              <button
                @click="openEditModal(user)"
                class="text-primary-600 hover:text-primary-900 mr-3"
              >
                <PencilIcon class="h-4 w-4" />
              </button>
              <button
                @click="confirmDelete(user)"
                class="text-red-600 hover:text-red-900"
              >
                <TrashIcon class="h-4 w-4" />
              </button>
            </td>
          </tr>
          <tr v-if="users.length === 0">
            <td colspan="4" class="px-6 py-12 text-center text-gray-500">
              No users found
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <!-- Create/Edit Modal -->
    <div v-if="showModal" class="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div class="bg-white rounded-lg p-6 max-w-md w-full mx-4">
        <div class="flex items-center justify-between mb-4">
          <h3 class="text-lg font-semibold text-gray-900">
            {{ modalMode === 'create' ? 'Create User' : 'Edit User' }}
          </h3>
          <button @click="closeModal" class="text-gray-400 hover:text-gray-600">
            <XMarkIcon class="h-5 w-5" />
          </button>
        </div>

        <form @submit.prevent="handleSubmit" class="space-y-4">
          <div>
            <label class="label">Username</label>
            <input
              v-model="form.username"
              type="text"
              required
              :disabled="modalMode === 'edit'"
              class="input"
              :class="{ 'bg-gray-100': modalMode === 'edit' }"
            />
          </div>

          <div>
            <label class="label">
              {{ modalMode === 'create' ? 'Password' : 'New Password (leave empty to keep current)' }}
            </label>
            <input
              v-model="form.password"
              type="password"
              :required="modalMode === 'create'"
              class="input"
            />
          </div>

          <div>
            <label class="label">Role</label>
            <select v-model="form.role" class="input">
              <option v-for="role in roles" :key="role.value" :value="role.value">
                {{ role.label }}
              </option>
            </select>
          </div>

          <div class="flex justify-end gap-3 pt-4">
            <button type="button" @click="closeModal" class="btn btn-secondary">
              Cancel
            </button>
            <button type="submit" :disabled="actionLoading" class="btn btn-primary">
              {{ actionLoading ? 'Saving...' : (modalMode === 'create' ? 'Create' : 'Save') }}
            </button>
          </div>
        </form>
      </div>
    </div>

    <!-- Delete confirmation modal -->
    <div v-if="showDeleteConfirm" class="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div class="bg-white rounded-lg p-6 max-w-md w-full mx-4">
        <h3 class="text-lg font-semibold text-gray-900 mb-4">Delete User?</h3>
        <p class="text-gray-600 mb-6">
          Are you sure you want to delete user "{{ deletingUser?.username }}"? This action cannot be undone.
        </p>
        <div class="flex justify-end gap-3">
          <button @click="showDeleteConfirm = false; deletingUser = null" class="btn btn-secondary">
            Cancel
          </button>
          <button @click="handleDelete" :disabled="actionLoading" class="btn btn-danger">
            {{ actionLoading ? 'Deleting...' : 'Delete' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
