<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue'
import { cacheApi, logsApi, type CacheStats, type CacheEntry, type ActivityLog } from '../api/client'
import {
  ArchiveBoxIcon,
  DocumentIcon,
  Square3Stack3DIcon,
  ArrowTrendingUpIcon,
  ArrowPathIcon,
  ClockIcon,
  PlayIcon,
  PauseIcon,
  ChartBarIcon
} from '@heroicons/vue/24/outline'

const stats = ref<CacheStats | null>(null)
const topAccessed = ref<CacheEntry[]>([])
const recentLogs = ref<ActivityLog[]>([])
const loading = ref(true)
const error = ref('')

// Auto-refresh
const autoRefresh = ref(false)
const refreshInterval = ref(30) // seconds
let refreshTimer: ReturnType<typeof setInterval> | null = null

async function fetchStats() {
  try {
    const response = await cacheApi.getStats()
    stats.value = response.data
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Failed to fetch stats'
  }
}

async function fetchTopAccessed() {
  try {
    const response = await cacheApi.getTopAccessed()
    topAccessed.value = response.data
  } catch (err: any) {
    // Non-critical
    console.error('Failed to fetch top accessed:', err)
  }
}

async function fetchRecentLogs() {
  try {
    const response = await logsApi.list({ limit: 5 })
    recentLogs.value = response.data.logs
  } catch (err: any) {
    // Non-critical
    console.error('Failed to fetch recent logs:', err)
  }
}

async function fetchAll() {
  loading.value = true
  error.value = ''
  await Promise.all([fetchStats(), fetchTopAccessed(), fetchRecentLogs()])
  loading.value = false
}

function toggleAutoRefresh() {
  autoRefresh.value = !autoRefresh.value
  if (autoRefresh.value) {
    startAutoRefresh()
  } else {
    stopAutoRefresh()
  }
}

function startAutoRefresh() {
  if (refreshTimer) clearInterval(refreshTimer)
  refreshTimer = setInterval(() => {
    if (!loading.value) {
      fetchAll()
    }
  }, refreshInterval.value * 1000)
}

function stopAutoRefresh() {
  if (refreshTimer) {
    clearInterval(refreshTimer)
    refreshTimer = null
  }
}

const hitRatePercent = computed(() => {
  if (!stats.value) return '0.0'
  return (stats.value.hit_rate * 100).toFixed(1)
})

const hitRateColor = computed(() => {
  if (!stats.value) return 'text-gray-600'
  const rate = stats.value.hit_rate * 100
  if (rate >= 80) return 'text-green-600'
  if (rate >= 50) return 'text-yellow-600'
  return 'text-red-600'
})

const storageUsagePercent = computed(() => {
  if (!stats.value) return 0
  // Assuming a 10GB default max size
  const maxSize = 10 * 1024 * 1024 * 1024
  return Math.min(100, (stats.value.total_size / maxSize) * 100)
})

function formatDate(dateStr: string) {
  const date = new Date(dateStr)
  const now = new Date()
  const diff = now.getTime() - date.getTime()

  if (diff < 60000) return 'Just now'
  if (diff < 3600000) return `${Math.floor(diff / 60000)} min ago`
  if (diff < 86400000) return `${Math.floor(diff / 3600000)} hours ago`
  return date.toLocaleDateString()
}

function truncateDigest(digest: string) {
  if (digest.length > 20) {
    return digest.substring(0, 20) + '...'
  }
  return digest
}

onMounted(fetchAll)

onUnmounted(() => {
  stopAutoRefresh()
})
</script>

<template>
  <div class="p-8">
    <div class="mb-8 flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold text-gray-900">Dashboard</h1>
        <p class="mt-1 text-sm text-gray-500">Overview of your Harbor Cache instance</p>
      </div>
      <div class="flex items-center gap-3">
        <button
          @click="toggleAutoRefresh"
          class="btn flex items-center gap-2"
          :class="autoRefresh ? 'btn-primary' : 'btn-secondary'"
        >
          <PlayIcon v-if="!autoRefresh" class="h-4 w-4" />
          <PauseIcon v-else class="h-4 w-4" />
          {{ autoRefresh ? 'Stop' : 'Auto' }} Refresh
        </button>
        <button
          @click="fetchAll"
          :disabled="loading"
          class="btn btn-secondary flex items-center gap-2"
        >
          <ArrowPathIcon class="h-4 w-4" :class="{ 'animate-spin': loading }" />
          Refresh
        </button>
      </div>
    </div>

    <!-- Auto-refresh indicator -->
    <div v-if="autoRefresh" class="mb-4 flex items-center gap-2 text-sm text-primary-600">
      <div class="h-2 w-2 bg-primary-500 rounded-full animate-pulse"></div>
      Auto-refreshing every {{ refreshInterval }} seconds
    </div>

    <!-- Error message -->
    <div v-if="error" class="mb-6 rounded-md bg-red-50 p-4">
      <p class="text-sm text-red-700">{{ error }}</p>
    </div>

    <!-- Loading state -->
    <div v-if="loading && !stats" class="flex items-center justify-center h-64">
      <ArrowPathIcon class="h-8 w-8 text-gray-400 animate-spin" />
    </div>

    <template v-else-if="stats">
      <!-- Stats grid -->
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6 mb-8">
        <!-- Total Size -->
        <div class="stat-card">
          <div class="flex items-center gap-3">
            <div class="p-2 bg-blue-100 rounded-lg">
              <ArchiveBoxIcon class="h-6 w-6 text-blue-600" />
            </div>
            <div>
              <p class="stat-label">Total Size</p>
              <p class="stat-value">{{ stats.total_size_human }}</p>
            </div>
          </div>
        </div>

        <!-- Entry Count -->
        <div class="stat-card">
          <div class="flex items-center gap-3">
            <div class="p-2 bg-green-100 rounded-lg">
              <Square3Stack3DIcon class="h-6 w-6 text-green-600" />
            </div>
            <div>
              <p class="stat-label">Total Entries</p>
              <p class="stat-value">{{ stats.entry_count }}</p>
            </div>
          </div>
        </div>

        <!-- Manifests -->
        <div class="stat-card">
          <div class="flex items-center gap-3">
            <div class="p-2 bg-purple-100 rounded-lg">
              <DocumentIcon class="h-6 w-6 text-purple-600" />
            </div>
            <div>
              <p class="stat-label">Manifests</p>
              <p class="stat-value">{{ stats.manifest_count }}</p>
            </div>
          </div>
        </div>

        <!-- Blobs -->
        <div class="stat-card">
          <div class="flex items-center gap-3">
            <div class="p-2 bg-orange-100 rounded-lg">
              <ArchiveBoxIcon class="h-6 w-6 text-orange-600" />
            </div>
            <div>
              <p class="stat-label">Blobs</p>
              <p class="stat-value">{{ stats.blob_count }}</p>
            </div>
          </div>
        </div>
      </div>

      <!-- Cache Performance & Storage -->
      <div class="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-8">
        <!-- Cache Performance -->
        <div class="card">
          <h2 class="text-lg font-semibold text-gray-900 mb-4 flex items-center gap-2">
            <ArrowTrendingUpIcon class="h-5 w-5 text-gray-500" />
            Cache Performance
          </h2>
          <div class="grid grid-cols-3 gap-6">
            <!-- Hit Rate -->
            <div class="text-center">
              <div class="relative inline-flex items-center justify-center">
                <svg class="w-20 h-20 transform -rotate-90">
                  <circle
                    cx="40"
                    cy="40"
                    r="35"
                    stroke="currentColor"
                    stroke-width="6"
                    fill="none"
                    class="text-gray-200"
                  />
                  <circle
                    cx="40"
                    cy="40"
                    r="35"
                    stroke="currentColor"
                    stroke-width="6"
                    fill="none"
                    :class="hitRateColor"
                    :stroke-dasharray="`${stats.hit_rate * 220} 220`"
                  />
                </svg>
                <span class="absolute text-lg font-bold" :class="hitRateColor">
                  {{ hitRatePercent }}%
                </span>
              </div>
              <p class="text-sm text-gray-500 mt-2">Hit Rate</p>
            </div>

            <!-- Hits -->
            <div class="text-center">
              <p class="text-3xl font-bold text-green-600">{{ stats.hit_count }}</p>
              <p class="text-sm text-gray-500">Cache Hits</p>
            </div>

            <!-- Misses -->
            <div class="text-center">
              <p class="text-3xl font-bold text-red-600">{{ stats.miss_count }}</p>
              <p class="text-sm text-gray-500">Cache Misses</p>
            </div>
          </div>
        </div>

        <!-- Storage Usage -->
        <div class="card">
          <h2 class="text-lg font-semibold text-gray-900 mb-4 flex items-center gap-2">
            <ChartBarIcon class="h-5 w-5 text-gray-500" />
            Storage Usage
          </h2>
          <div class="space-y-4">
            <div>
              <div class="flex justify-between text-sm mb-1">
                <span class="text-gray-600">Used Space</span>
                <span class="font-medium text-gray-900">{{ stats.total_size_human }}</span>
              </div>
              <div class="w-full bg-gray-200 rounded-full h-3">
                <div
                  class="h-3 rounded-full transition-all duration-500"
                  :class="storageUsagePercent > 80 ? 'bg-red-500' : storageUsagePercent > 50 ? 'bg-yellow-500' : 'bg-green-500'"
                  :style="{ width: `${storageUsagePercent}%` }"
                ></div>
              </div>
            </div>
            <div class="grid grid-cols-2 gap-4 pt-4">
              <div class="bg-purple-50 rounded-lg p-3">
                <p class="text-sm text-purple-600">Manifests</p>
                <p class="text-xl font-bold text-purple-700">{{ stats.manifest_count }}</p>
              </div>
              <div class="bg-blue-50 rounded-lg p-3">
                <p class="text-sm text-blue-600">Blobs</p>
                <p class="text-xl font-bold text-blue-700">{{ stats.blob_count }}</p>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Bottom Section -->
      <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <!-- Top Accessed -->
        <div class="card">
          <h2 class="text-lg font-semibold text-gray-900 mb-4 flex items-center gap-2">
            <ArrowTrendingUpIcon class="h-5 w-5 text-gray-500" />
            Most Accessed Items
          </h2>
          <div v-if="topAccessed.length > 0" class="space-y-3">
            <div
              v-for="(entry, index) in topAccessed.slice(0, 5)"
              :key="entry.id"
              class="flex items-center gap-3 p-2 rounded-lg hover:bg-gray-50"
            >
              <div
                class="w-6 h-6 rounded-full flex items-center justify-center text-xs font-bold"
                :class="index === 0 ? 'bg-yellow-100 text-yellow-700' : index === 1 ? 'bg-gray-200 text-gray-600' : index === 2 ? 'bg-orange-100 text-orange-700' : 'bg-gray-100 text-gray-500'"
              >
                {{ index + 1 }}
              </div>
              <div class="flex-1 min-w-0">
                <p class="text-sm font-medium text-gray-900 truncate">
                  {{ entry.repository || 'Unknown' }}
                </p>
                <p class="text-xs text-gray-500 font-mono truncate">
                  {{ truncateDigest(entry.digest) }}
                </p>
              </div>
              <div class="text-right">
                <p class="text-sm font-bold text-gray-900">{{ entry.access_count }}</p>
                <p class="text-xs text-gray-500">accesses</p>
              </div>
            </div>
          </div>
          <div v-else class="text-center text-gray-500 py-8">
            No cached items yet
          </div>
        </div>

        <!-- Recent Activity -->
        <div class="card">
          <h2 class="text-lg font-semibold text-gray-900 mb-4 flex items-center gap-2">
            <ClockIcon class="h-5 w-5 text-gray-500" />
            Recent Activity
          </h2>
          <div v-if="recentLogs.length > 0" class="space-y-3">
            <div
              v-for="log in recentLogs"
              :key="log.id"
              class="flex items-start gap-3 p-2 rounded-lg hover:bg-gray-50"
            >
              <div class="w-2 h-2 rounded-full bg-primary-500 mt-2 flex-shrink-0"></div>
              <div class="flex-1 min-w-0">
                <p class="text-sm text-gray-900">
                  <span class="font-medium">{{ log.action }}</span>
                  <span class="text-gray-500"> on </span>
                  <span class="font-mono text-xs">{{ log.resource_type }}</span>
                </p>
                <p class="text-xs text-gray-500">
                  {{ log.username || 'System' }} - {{ formatDate(log.timestamp) }}
                </p>
              </div>
            </div>
          </div>
          <div v-else class="text-center text-gray-500 py-8">
            No recent activity
          </div>
        </div>
      </div>
    </template>
  </div>
</template>
