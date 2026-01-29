<script setup lang="ts">
import { ref, onMounted, computed, watch } from 'vue'
import { cacheApi, type CacheStats, type CacheEntry, type CacheEntriesQuery } from '../api/client'
import { useAuthStore } from '../stores/auth'
import {
  ArrowPathIcon,
  TrashIcon,
  ClockIcon,
  ExclamationTriangleIcon,
  MagnifyingGlassIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  FunnelIcon
} from '@heroicons/vue/24/outline'

const authStore = useAuthStore()
const stats = ref<CacheStats | null>(null)
const entries = ref<CacheEntry[]>([])
const topAccessed = ref<CacheEntry[]>([])
const repositories = ref<string[]>([])
const loading = ref(true)
const entriesLoading = ref(false)
const actionLoading = ref(false)
const error = ref('')
const success = ref('')
const showClearConfirm = ref(false)
const showDeleteConfirm = ref(false)
const deletingEntry = ref<CacheEntry | null>(null)

// Pagination and filtering
const currentPage = ref(1)
const pageSize = ref(20)
const totalEntries = ref(0)
const filters = ref<CacheEntriesQuery>({
  entry_type: '',
  repository: '',
  digest: '',
  sort_by: 'last_accessed_at',
  sort_order: 'desc'
})
const showFilters = ref(false)

const totalPages = computed(() => Math.ceil(totalEntries.value / pageSize.value))

async function fetchStats() {
  try {
    const response = await cacheApi.getStats()
    stats.value = response.data
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Failed to fetch stats'
  }
}

async function fetchEntries() {
  entriesLoading.value = true
  try {
    const query: CacheEntriesQuery = {
      offset: (currentPage.value - 1) * pageSize.value,
      limit: pageSize.value,
      sort_by: filters.value.sort_by,
      sort_order: filters.value.sort_order
    }
    if (filters.value.entry_type) query.entry_type = filters.value.entry_type
    if (filters.value.repository) query.repository = filters.value.repository
    if (filters.value.digest) query.digest = filters.value.digest

    const response = await cacheApi.getEntries(query)
    entries.value = response.data.entries
    totalEntries.value = response.data.total
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Failed to fetch entries'
  } finally {
    entriesLoading.value = false
  }
}

async function fetchTopAccessed() {
  try {
    const response = await cacheApi.getTopAccessed()
    topAccessed.value = response.data
  } catch (err: any) {
    // Non-critical, just log
    console.error('Failed to fetch top accessed:', err)
  }
}

async function fetchRepositories() {
  try {
    const response = await cacheApi.getRepositories()
    repositories.value = response.data.repositories
  } catch (err: any) {
    // Non-critical, just log
    console.error('Failed to fetch repositories:', err)
  }
}

async function fetchAll() {
  loading.value = true
  error.value = ''
  await Promise.all([fetchStats(), fetchEntries(), fetchTopAccessed(), fetchRepositories()])
  loading.value = false
}

async function handleCleanup() {
  actionLoading.value = true
  error.value = ''
  success.value = ''
  try {
    const response = await cacheApi.cleanup()
    success.value = `Cleaned up ${response.data.cleaned} expired entries`
    await fetchAll()
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Cleanup failed'
  } finally {
    actionLoading.value = false
  }
}

async function handleClear() {
  actionLoading.value = true
  error.value = ''
  success.value = ''
  showClearConfirm.value = false
  try {
    const response = await cacheApi.clear()
    success.value = `Cleared ${response.data.cleared} entries from cache`
    await fetchAll()
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Clear failed'
  } finally {
    actionLoading.value = false
  }
}

function confirmDeleteEntry(entry: CacheEntry) {
  deletingEntry.value = entry
  showDeleteConfirm.value = true
}

async function handleDeleteEntry() {
  if (!deletingEntry.value) return
  actionLoading.value = true
  error.value = ''
  success.value = ''
  try {
    await cacheApi.deleteEntry(deletingEntry.value.digest)
    success.value = `Deleted cache entry: ${deletingEntry.value.digest.substring(0, 20)}...`
    showDeleteConfirm.value = false
    deletingEntry.value = null
    await fetchAll()
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Delete failed'
  } finally {
    actionLoading.value = false
  }
}

function applyFilters() {
  currentPage.value = 1
  fetchEntries()
}

function clearFilters() {
  filters.value = {
    entry_type: '',
    repository: '',
    digest: '',
    sort_by: 'last_accessed_at',
    sort_order: 'desc'
  }
  currentPage.value = 1
  fetchEntries()
}

function goToPage(page: number) {
  if (page >= 1 && page <= totalPages.value) {
    currentPage.value = page
    fetchEntries()
  }
}

function formatDate(dateStr: string) {
  return new Date(dateStr).toLocaleString()
}

function truncateDigest(digest: string) {
  if (digest.length > 25) {
    return digest.substring(0, 25) + '...'
  }
  return digest
}

const hitRatePercent = computed(() => {
  if (!stats.value) return '0.0'
  return (stats.value.hit_rate * 100).toFixed(1)
})

watch(() => pageSize.value, () => {
  currentPage.value = 1
  fetchEntries()
})

onMounted(fetchAll)
</script>

<template>
  <div class="p-8">
    <div class="mb-8 flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold text-gray-900">Cache Management</h1>
        <p class="mt-1 text-sm text-gray-500">View and manage cached artifacts</p>
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

    <template v-else-if="stats">
      <!-- Cache Statistics -->
      <div class="card mb-6">
        <h2 class="text-lg font-semibold text-gray-900 mb-4">Cache Statistics</h2>
        <div class="grid grid-cols-2 md:grid-cols-5 gap-4">
          <div>
            <p class="text-sm text-gray-500">Total Size</p>
            <p class="text-xl font-bold text-gray-900">{{ stats.total_size_human }}</p>
          </div>
          <div>
            <p class="text-sm text-gray-500">Total Entries</p>
            <p class="text-xl font-bold text-gray-900">{{ stats.entry_count }}</p>
          </div>
          <div>
            <p class="text-sm text-gray-500">Manifests</p>
            <p class="text-xl font-bold text-gray-900">{{ stats.manifest_count }}</p>
          </div>
          <div>
            <p class="text-sm text-gray-500">Blobs</p>
            <p class="text-xl font-bold text-gray-900">{{ stats.blob_count }}</p>
          </div>
          <div>
            <p class="text-sm text-gray-500">Hit Rate</p>
            <p class="text-xl font-bold text-green-600">{{ hitRatePercent }}%</p>
          </div>
        </div>
      </div>

      <!-- Top Accessed Items -->
      <div v-if="topAccessed.length > 0" class="card mb-6">
        <h2 class="text-lg font-semibold text-gray-900 mb-4">Most Frequently Accessed</h2>
        <div class="overflow-x-auto">
          <table class="min-w-full divide-y divide-gray-200">
            <thead class="bg-gray-50">
              <tr>
                <th class="px-4 py-2 text-left text-xs font-medium text-gray-500 uppercase">Type</th>
                <th class="px-4 py-2 text-left text-xs font-medium text-gray-500 uppercase">Repository</th>
                <th class="px-4 py-2 text-left text-xs font-medium text-gray-500 uppercase">Digest</th>
                <th class="px-4 py-2 text-left text-xs font-medium text-gray-500 uppercase">Size</th>
                <th class="px-4 py-2 text-left text-xs font-medium text-gray-500 uppercase">Access Count</th>
              </tr>
            </thead>
            <tbody class="bg-white divide-y divide-gray-200">
              <tr v-for="entry in topAccessed.slice(0, 5)" :key="entry.id">
                <td class="px-4 py-2 whitespace-nowrap">
                  <span
                    class="px-2 py-1 text-xs font-medium rounded-full"
                    :class="entry.entry_type === 'manifest' ? 'bg-purple-100 text-purple-800' : 'bg-blue-100 text-blue-800'"
                  >
                    {{ entry.entry_type }}
                  </span>
                </td>
                <td class="px-4 py-2 whitespace-nowrap text-sm text-gray-700">
                  {{ entry.repository || '-' }}
                </td>
                <td class="px-4 py-2 whitespace-nowrap text-sm font-mono text-gray-500">
                  {{ truncateDigest(entry.digest) }}
                </td>
                <td class="px-4 py-2 whitespace-nowrap text-sm text-gray-700">
                  {{ entry.size_human }}
                </td>
                <td class="px-4 py-2 whitespace-nowrap text-sm font-bold text-gray-900">
                  {{ entry.access_count }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>

      <!-- Cache Entries Table -->
      <div class="card mb-6">
        <div class="flex items-center justify-between mb-4">
          <h2 class="text-lg font-semibold text-gray-900">Cached Entries</h2>
          <button
            @click="showFilters = !showFilters"
            class="btn btn-secondary flex items-center gap-2 text-sm"
          >
            <FunnelIcon class="h-4 w-4" />
            Filters
          </button>
        </div>

        <!-- Filters Panel -->
        <div v-if="showFilters" class="bg-gray-50 rounded-lg p-4 mb-4">
          <div class="grid grid-cols-1 md:grid-cols-4 gap-4">
            <div>
              <label class="label">Type</label>
              <select v-model="filters.entry_type" class="input text-sm">
                <option value="">All</option>
                <option value="manifest">Manifest</option>
                <option value="blob">Blob</option>
              </select>
            </div>
            <div>
              <label class="label">Repository</label>
              <select v-model="filters.repository" class="input text-sm">
                <option value="">All</option>
                <option v-for="repo in repositories" :key="repo" :value="repo">{{ repo }}</option>
              </select>
            </div>
            <div>
              <label class="label">Digest (contains)</label>
              <div class="relative">
                <MagnifyingGlassIcon class="h-4 w-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
                <input
                  v-model="filters.digest"
                  type="text"
                  placeholder="Search digest..."
                  class="input text-sm pl-9"
                />
              </div>
            </div>
            <div>
              <label class="label">Sort By</label>
              <select v-model="filters.sort_by" class="input text-sm">
                <option value="last_accessed_at">Last Accessed</option>
                <option value="created_at">Created</option>
                <option value="size">Size</option>
                <option value="access_count">Access Count</option>
              </select>
            </div>
          </div>
          <div class="flex justify-end gap-2 mt-4">
            <button @click="clearFilters" class="btn btn-secondary text-sm">Clear</button>
            <button @click="applyFilters" class="btn btn-primary text-sm">Apply</button>
          </div>
        </div>

        <!-- Entries Table -->
        <div v-if="entriesLoading" class="flex items-center justify-center h-32">
          <ArrowPathIcon class="h-6 w-6 text-gray-400 animate-spin" />
        </div>
        <template v-else>
          <div class="overflow-x-auto">
            <table class="min-w-full divide-y divide-gray-200">
              <thead class="bg-gray-50">
                <tr>
                  <th class="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Type</th>
                  <th class="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Repository</th>
                  <th class="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Digest</th>
                  <th class="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Size</th>
                  <th class="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Last Accessed</th>
                  <th class="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Accesses</th>
                  <th v-if="authStore.isAdmin" class="px-4 py-3 text-right text-xs font-medium text-gray-500 uppercase">Actions</th>
                </tr>
              </thead>
              <tbody class="bg-white divide-y divide-gray-200">
                <tr v-for="entry in entries" :key="entry.id" class="hover:bg-gray-50">
                  <td class="px-4 py-3 whitespace-nowrap">
                    <span
                      class="px-2 py-1 text-xs font-medium rounded-full"
                      :class="entry.entry_type === 'manifest' ? 'bg-purple-100 text-purple-800' : 'bg-blue-100 text-blue-800'"
                    >
                      {{ entry.entry_type }}
                    </span>
                  </td>
                  <td class="px-4 py-3 whitespace-nowrap text-sm text-gray-700">
                    {{ entry.repository || '-' }}
                  </td>
                  <td class="px-4 py-3 whitespace-nowrap">
                    <span class="text-xs font-mono text-gray-500" :title="entry.digest">
                      {{ truncateDigest(entry.digest) }}
                    </span>
                  </td>
                  <td class="px-4 py-3 whitespace-nowrap text-sm text-gray-700">
                    {{ entry.size_human }}
                  </td>
                  <td class="px-4 py-3 whitespace-nowrap text-sm text-gray-500">
                    {{ formatDate(entry.last_accessed_at) }}
                  </td>
                  <td class="px-4 py-3 whitespace-nowrap text-sm text-gray-700">
                    {{ entry.access_count }}
                  </td>
                  <td v-if="authStore.isAdmin" class="px-4 py-3 whitespace-nowrap text-right">
                    <button
                      @click="confirmDeleteEntry(entry)"
                      class="text-red-600 hover:text-red-900"
                      title="Delete entry"
                    >
                      <TrashIcon class="h-4 w-4" />
                    </button>
                  </td>
                </tr>
                <tr v-if="entries.length === 0">
                  <td :colspan="authStore.isAdmin ? 7 : 6" class="px-4 py-12 text-center text-gray-500">
                    No cache entries found
                  </td>
                </tr>
              </tbody>
            </table>
          </div>

          <!-- Pagination -->
          <div v-if="totalPages > 1" class="flex items-center justify-between mt-4 pt-4 border-t">
            <div class="text-sm text-gray-500">
              Showing {{ (currentPage - 1) * pageSize + 1 }} to {{ Math.min(currentPage * pageSize, totalEntries) }} of {{ totalEntries }} entries
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
        </template>
      </div>

      <!-- Cache Actions -->
      <div v-if="authStore.isAdmin" class="card">
        <h2 class="text-lg font-semibold text-gray-900 mb-4">Cache Actions</h2>
        <div class="space-y-4">
          <!-- Cleanup expired -->
          <div class="flex items-center justify-between p-4 bg-gray-50 rounded-lg">
            <div class="flex items-center gap-3">
              <div class="p-2 bg-blue-100 rounded-lg">
                <ClockIcon class="h-6 w-6 text-blue-600" />
              </div>
              <div>
                <p class="font-medium text-gray-900">Cleanup Expired Entries</p>
                <p class="text-sm text-gray-500">Remove entries older than the retention period</p>
              </div>
            </div>
            <button
              @click="handleCleanup"
              :disabled="actionLoading"
              class="btn btn-secondary"
            >
              {{ actionLoading ? 'Running...' : 'Run Cleanup' }}
            </button>
          </div>

          <!-- Clear all -->
          <div class="flex items-center justify-between p-4 bg-red-50 rounded-lg">
            <div class="flex items-center gap-3">
              <div class="p-2 bg-red-100 rounded-lg">
                <TrashIcon class="h-6 w-6 text-red-600" />
              </div>
              <div>
                <p class="font-medium text-gray-900">Clear All Cache</p>
                <p class="text-sm text-gray-500">Remove all cached entries (this cannot be undone)</p>
              </div>
            </div>
            <button
              @click="showClearConfirm = true"
              :disabled="actionLoading"
              class="btn btn-danger"
            >
              Clear Cache
            </button>
          </div>
        </div>
      </div>
    </template>

    <!-- Clear confirmation modal -->
    <div v-if="showClearConfirm" class="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div class="bg-white rounded-lg p-6 max-w-md w-full mx-4">
        <div class="flex items-center gap-3 mb-4">
          <div class="p-2 bg-red-100 rounded-full">
            <ExclamationTriangleIcon class="h-6 w-6 text-red-600" />
          </div>
          <h3 class="text-lg font-semibold text-gray-900">Clear All Cache?</h3>
        </div>
        <p class="text-gray-600 mb-6">
          This will remove all cached manifests and blobs. This action cannot be undone.
        </p>
        <div class="flex justify-end gap-3">
          <button @click="showClearConfirm = false" class="btn btn-secondary">
            Cancel
          </button>
          <button @click="handleClear" class="btn btn-danger">
            Clear All
          </button>
        </div>
      </div>
    </div>

    <!-- Delete entry confirmation modal -->
    <div v-if="showDeleteConfirm" class="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div class="bg-white rounded-lg p-6 max-w-md w-full mx-4">
        <div class="flex items-center gap-3 mb-4">
          <div class="p-2 bg-red-100 rounded-full">
            <ExclamationTriangleIcon class="h-6 w-6 text-red-600" />
          </div>
          <h3 class="text-lg font-semibold text-gray-900">Delete Cache Entry?</h3>
        </div>
        <p class="text-gray-600 mb-2">
          Are you sure you want to delete this cache entry?
        </p>
        <p class="text-sm font-mono text-gray-500 bg-gray-100 p-2 rounded mb-6 break-all">
          {{ deletingEntry?.digest }}
        </p>
        <div class="flex justify-end gap-3">
          <button @click="showDeleteConfirm = false; deletingEntry = null" class="btn btn-secondary">
            Cancel
          </button>
          <button @click="handleDeleteEntry" :disabled="actionLoading" class="btn btn-danger">
            {{ actionLoading ? 'Deleting...' : 'Delete' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
