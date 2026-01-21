<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { configApi, type ConfigEntry } from '../api/client'
import {
  ArrowPathIcon,
  PlusIcon,
  TrashIcon,
  XMarkIcon
} from '@heroicons/vue/24/outline'

const entries = ref<ConfigEntry[]>([])
const loading = ref(true)
const actionLoading = ref(false)
const error = ref('')
const success = ref('')

// Modal state
const showModal = ref(false)
const editedEntries = ref<Array<{ key: string; value: string; isNew?: boolean }>>([])

async function fetchConfig() {
  loading.value = true
  error.value = ''
  try {
    const response = await configApi.list()
    entries.value = response.data
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Failed to fetch config'
  } finally {
    loading.value = false
  }
}

function openEditModal() {
  editedEntries.value = entries.value.map(e => ({ key: e.key, value: e.value }))
  showModal.value = true
}

function closeModal() {
  showModal.value = false
  editedEntries.value = []
}

function addEntry() {
  editedEntries.value.push({ key: '', value: '', isNew: true })
}

function removeEntry(index: number) {
  editedEntries.value.splice(index, 1)
}

async function handleSave() {
  // Validate entries
  const validEntries = editedEntries.value.filter(e => e.key.trim() && e.value.trim())
  if (validEntries.length === 0) {
    error.value = 'At least one valid config entry is required'
    return
  }

  actionLoading.value = true
  error.value = ''
  success.value = ''

  try {
    await configApi.update({
      entries: validEntries.map(e => ({ key: e.key.trim(), value: e.value.trim() }))
    })
    success.value = 'Configuration saved successfully'
    closeModal()
    await fetchConfig()
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Save failed'
  } finally {
    actionLoading.value = false
  }
}

async function handleDelete(key: string) {
  if (!confirm(`Delete config key "${key}"?`)) return

  actionLoading.value = true
  error.value = ''
  success.value = ''

  try {
    await configApi.delete(key)
    success.value = `Config key "${key}" deleted`
    await fetchConfig()
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Delete failed'
  } finally {
    actionLoading.value = false
  }
}

function formatDate(dateStr: string) {
  return new Date(dateStr).toLocaleString()
}

onMounted(fetchConfig)
</script>

<template>
  <div class="p-8">
    <div class="mb-8 flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold text-gray-900">Configuration</h1>
        <p class="mt-1 text-sm text-gray-500">Manage runtime configuration settings</p>
      </div>
      <div class="flex gap-2">
        <button @click="fetchConfig" :disabled="loading" class="btn btn-secondary flex items-center gap-2">
          <ArrowPathIcon class="h-4 w-4" :class="{ 'animate-spin': loading }" />
          Refresh
        </button>
        <button @click="openEditModal" class="btn btn-primary flex items-center gap-2">
          <PlusIcon class="h-4 w-4" />
          Edit Config
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

    <!-- Config entries -->
    <div v-else class="card p-0 overflow-hidden">
      <table class="min-w-full divide-y divide-gray-200">
        <thead class="bg-gray-50">
          <tr>
            <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
              Key
            </th>
            <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
              Value
            </th>
            <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
              Updated
            </th>
            <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase tracking-wider">
              Actions
            </th>
          </tr>
        </thead>
        <tbody class="bg-white divide-y divide-gray-200">
          <tr v-for="entry in entries" :key="entry.key">
            <td class="px-6 py-4 whitespace-nowrap">
              <code class="text-sm font-mono text-gray-900 bg-gray-100 px-2 py-1 rounded">
                {{ entry.key }}
              </code>
            </td>
            <td class="px-6 py-4">
              <span class="text-sm text-gray-700">{{ entry.value }}</span>
            </td>
            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-500">
              {{ formatDate(entry.updated_at) }}
            </td>
            <td class="px-6 py-4 whitespace-nowrap text-right text-sm font-medium">
              <button
                @click="handleDelete(entry.key)"
                class="text-red-600 hover:text-red-900"
              >
                <TrashIcon class="h-4 w-4" />
              </button>
            </td>
          </tr>
          <tr v-if="entries.length === 0">
            <td colspan="4" class="px-6 py-12 text-center text-gray-500">
              No configuration entries found. Click "Edit Config" to add some.
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <!-- Edit Modal -->
    <div v-if="showModal" class="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div class="bg-white rounded-lg p-6 max-w-2xl w-full mx-4 max-h-[80vh] overflow-auto">
        <div class="flex items-center justify-between mb-4">
          <h3 class="text-lg font-semibold text-gray-900">Edit Configuration</h3>
          <button @click="closeModal" class="text-gray-400 hover:text-gray-600">
            <XMarkIcon class="h-5 w-5" />
          </button>
        </div>

        <div class="space-y-3">
          <div
            v-for="(entry, index) in editedEntries"
            :key="index"
            class="flex gap-3 items-start"
          >
            <div class="flex-1">
              <input
                v-model="entry.key"
                type="text"
                placeholder="Key"
                class="input font-mono text-sm"
                :disabled="!entry.isNew"
                :class="{ 'bg-gray-100': !entry.isNew }"
              />
            </div>
            <div class="flex-1">
              <input
                v-model="entry.value"
                type="text"
                placeholder="Value"
                class="input text-sm"
              />
            </div>
            <button
              @click="removeEntry(index)"
              class="p-2 text-red-600 hover:text-red-900"
            >
              <TrashIcon class="h-5 w-5" />
            </button>
          </div>

          <button
            @click="addEntry"
            class="flex items-center gap-2 text-sm text-primary-600 hover:text-primary-700"
          >
            <PlusIcon class="h-4 w-4" />
            Add Entry
          </button>
        </div>

        <div class="flex justify-end gap-3 pt-6 mt-6 border-t">
          <button @click="closeModal" class="btn btn-secondary">
            Cancel
          </button>
          <button @click="handleSave" :disabled="actionLoading" class="btn btn-primary">
            {{ actionLoading ? 'Saving...' : 'Save Changes' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
