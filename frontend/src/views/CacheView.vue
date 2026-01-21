<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { cacheApi, type CacheStats } from '../api/client'
import { useAuthStore } from '../stores/auth'
import {
  ArrowPathIcon,
  TrashIcon,
  ClockIcon,
  ExclamationTriangleIcon
} from '@heroicons/vue/24/outline'

const authStore = useAuthStore()
const stats = ref<CacheStats | null>(null)
const loading = ref(true)
const actionLoading = ref(false)
const error = ref('')
const success = ref('')
const showClearConfirm = ref(false)

async function fetchStats() {
  loading.value = true
  error.value = ''
  try {
    const response = await cacheApi.getStats()
    stats.value = response.data
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Failed to fetch stats'
  } finally {
    loading.value = false
  }
}

async function handleCleanup() {
  actionLoading.value = true
  error.value = ''
  success.value = ''
  try {
    const response = await cacheApi.cleanup()
    success.value = `Cleaned up ${response.data.cleaned} expired entries`
    await fetchStats()
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
    await fetchStats()
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Clear failed'
  } finally {
    actionLoading.value = false
  }
}

onMounted(fetchStats)
</script>

<template>
  <div class="p-8">
    <div class="mb-8 flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold text-gray-900">Cache Management</h1>
        <p class="mt-1 text-sm text-gray-500">View and manage cached artifacts</p>
      </div>
      <button
        @click="fetchStats"
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
        <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
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
        </div>
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
  </div>
</template>
