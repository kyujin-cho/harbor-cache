<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { logsApi, type ActivityLog, type ActivityLogsQuery } from '../api/client'
import {
  ArrowPathIcon,
  FunnelIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  ClockIcon
} from '@heroicons/vue/24/outline'

const logs = ref<ActivityLog[]>([])
const actionTypes = ref<string[]>([])
const resourceTypes = ref<string[]>([])
const loading = ref(true)
const error = ref('')

// Pagination and filtering
const currentPage = ref(1)
const pageSize = ref(20)
const totalLogs = ref(0)
const filters = ref<ActivityLogsQuery>({
  action: '',
  resource_type: '',
  start_date: '',
  end_date: ''
})
const showFilters = ref(false)

const totalPages = computed(() => Math.ceil(totalLogs.value / pageSize.value))

async function fetchLogs() {
  loading.value = true
  error.value = ''
  try {
    const query: ActivityLogsQuery = {
      offset: (currentPage.value - 1) * pageSize.value,
      limit: pageSize.value
    }
    if (filters.value.action) query.action = filters.value.action
    if (filters.value.resource_type) query.resource_type = filters.value.resource_type
    if (filters.value.start_date) query.start_date = new Date(filters.value.start_date).toISOString()
    if (filters.value.end_date) query.end_date = new Date(filters.value.end_date).toISOString()

    const response = await logsApi.list(query)
    logs.value = response.data.logs
    totalLogs.value = response.data.total
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Failed to fetch logs'
  } finally {
    loading.value = false
  }
}

async function fetchFilterOptions() {
  try {
    const [actionsResponse, resourceTypesResponse] = await Promise.all([
      logsApi.getActions(),
      logsApi.getResourceTypes()
    ])
    actionTypes.value = actionsResponse.data
    resourceTypes.value = resourceTypesResponse.data
  } catch (err: any) {
    console.error('Failed to fetch filter options:', err)
  }
}

async function fetchAll() {
  await Promise.all([fetchLogs(), fetchFilterOptions()])
}

function applyFilters() {
  currentPage.value = 1
  fetchLogs()
}

function clearFilters() {
  filters.value = {
    action: '',
    resource_type: '',
    start_date: '',
    end_date: ''
  }
  currentPage.value = 1
  fetchLogs()
}

function goToPage(page: number) {
  if (page >= 1 && page <= totalPages.value) {
    currentPage.value = page
    fetchLogs()
  }
}

function formatDate(dateStr: string) {
  return new Date(dateStr).toLocaleString()
}

function formatRelativeTime(dateStr: string) {
  const date = new Date(dateStr)
  const now = new Date()
  const diff = now.getTime() - date.getTime()

  if (diff < 60000) return 'Just now'
  if (diff < 3600000) return `${Math.floor(diff / 60000)} min ago`
  if (diff < 86400000) return `${Math.floor(diff / 3600000)} hours ago`
  if (diff < 604800000) return `${Math.floor(diff / 86400000)} days ago`
  return date.toLocaleDateString()
}

function getActionColor(action: string): string {
  const lowerAction = action.toLowerCase()
  if (lowerAction.includes('delete') || lowerAction.includes('clear')) {
    return 'bg-red-100 text-red-800'
  }
  if (lowerAction.includes('create') || lowerAction.includes('add')) {
    return 'bg-green-100 text-green-800'
  }
  if (lowerAction.includes('update') || lowerAction.includes('edit')) {
    return 'bg-blue-100 text-blue-800'
  }
  if (lowerAction.includes('login') || lowerAction.includes('auth')) {
    return 'bg-purple-100 text-purple-800'
  }
  return 'bg-gray-100 text-gray-800'
}

onMounted(fetchAll)
</script>

<template>
  <div class="p-8">
    <div class="mb-8 flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold text-gray-900">Activity Logs</h1>
        <p class="mt-1 text-sm text-gray-500">View system activity and audit logs</p>
      </div>
      <button
        @click="fetchAll"
        :disabled="loading"
        class="btn btn-secondary flex items-center gap-2"
      >
        <ArrowPathIcon class="h-4 w-4" :class="{ 'animate-spin': loading }" />
        Refresh
      </button>
    </div>

    <!-- Error message -->
    <div v-if="error" class="mb-6 rounded-md bg-red-50 p-4">
      <p class="text-sm text-red-700">{{ error }}</p>
    </div>

    <!-- Filters Card -->
    <div class="card mb-6">
      <div class="flex items-center justify-between mb-4">
        <h2 class="text-lg font-semibold text-gray-900">Filters</h2>
        <button
          @click="showFilters = !showFilters"
          class="btn btn-secondary flex items-center gap-2 text-sm"
        >
          <FunnelIcon class="h-4 w-4" />
          {{ showFilters ? 'Hide' : 'Show' }} Filters
        </button>
      </div>

      <div v-if="showFilters" class="space-y-4">
        <div class="grid grid-cols-1 md:grid-cols-4 gap-4">
          <div>
            <label class="label">Action</label>
            <select v-model="filters.action" class="input text-sm">
              <option value="">All Actions</option>
              <option v-for="action in actionTypes" :key="action" :value="action">{{ action }}</option>
            </select>
          </div>
          <div>
            <label class="label">Resource Type</label>
            <select v-model="filters.resource_type" class="input text-sm">
              <option value="">All Types</option>
              <option v-for="type in resourceTypes" :key="type" :value="type">{{ type }}</option>
            </select>
          </div>
          <div>
            <label class="label">Start Date</label>
            <input
              v-model="filters.start_date"
              type="date"
              class="input text-sm"
            />
          </div>
          <div>
            <label class="label">End Date</label>
            <input
              v-model="filters.end_date"
              type="date"
              class="input text-sm"
            />
          </div>
        </div>
        <div class="flex justify-end gap-2">
          <button @click="clearFilters" class="btn btn-secondary text-sm">Clear</button>
          <button @click="applyFilters" class="btn btn-primary text-sm">Apply Filters</button>
        </div>
      </div>
    </div>

    <!-- Loading state -->
    <div v-if="loading" class="flex items-center justify-center h-64">
      <ArrowPathIcon class="h-8 w-8 text-gray-400 animate-spin" />
    </div>

    <!-- Logs List -->
    <div v-else class="card p-0 overflow-hidden">
      <div class="overflow-x-auto">
        <table class="min-w-full divide-y divide-gray-200">
          <thead class="bg-gray-50">
            <tr>
              <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
                Timestamp
              </th>
              <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
                Action
              </th>
              <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
                Resource
              </th>
              <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
                User
              </th>
              <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
                Details
              </th>
            </tr>
          </thead>
          <tbody class="bg-white divide-y divide-gray-200">
            <tr v-for="log in logs" :key="log.id" class="hover:bg-gray-50">
              <td class="px-6 py-4 whitespace-nowrap">
                <div class="flex items-center gap-2">
                  <ClockIcon class="h-4 w-4 text-gray-400" />
                  <div>
                    <p class="text-sm text-gray-900">{{ formatRelativeTime(log.timestamp) }}</p>
                    <p class="text-xs text-gray-500">{{ formatDate(log.timestamp) }}</p>
                  </div>
                </div>
              </td>
              <td class="px-6 py-4 whitespace-nowrap">
                <span
                  class="px-2 py-1 text-xs font-medium rounded-full"
                  :class="getActionColor(log.action)"
                >
                  {{ log.action }}
                </span>
              </td>
              <td class="px-6 py-4 whitespace-nowrap">
                <div>
                  <p class="text-sm font-medium text-gray-900">{{ log.resource_type }}</p>
                  <p v-if="log.resource_id" class="text-xs text-gray-500 font-mono">
                    {{ log.resource_id.length > 30 ? log.resource_id.substring(0, 30) + '...' : log.resource_id }}
                  </p>
                </div>
              </td>
              <td class="px-6 py-4 whitespace-nowrap">
                <p class="text-sm text-gray-900">{{ log.username || 'System' }}</p>
                <p v-if="log.ip_address" class="text-xs text-gray-500">{{ log.ip_address }}</p>
              </td>
              <td class="px-6 py-4">
                <p v-if="log.details" class="text-sm text-gray-700 max-w-xs truncate" :title="log.details">
                  {{ log.details }}
                </p>
                <p v-else class="text-sm text-gray-400">-</p>
              </td>
            </tr>
            <tr v-if="logs.length === 0">
              <td colspan="5" class="px-6 py-12 text-center text-gray-500">
                No activity logs found
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <!-- Pagination -->
      <div v-if="totalPages > 1" class="flex items-center justify-between px-6 py-4 border-t">
        <div class="text-sm text-gray-500">
          Showing {{ (currentPage - 1) * pageSize + 1 }} to {{ Math.min(currentPage * pageSize, totalLogs) }} of {{ totalLogs }} logs
        </div>
        <div class="flex items-center gap-2">
          <button
            @click="goToPage(currentPage - 1)"
            :disabled="currentPage === 1"
            class="btn btn-secondary p-2"
          >
            <ChevronLeftIcon class="h-4 w-4" />
          </button>
          <span class="text-sm text-gray-700">Page {{ currentPage }} of {{ totalPages }}</span>
          <button
            @click="goToPage(currentPage + 1)"
            :disabled="currentPage === totalPages"
            class="btn btn-secondary p-2"
          >
            <ChevronRightIcon class="h-4 w-4" />
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
