<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { cacheApi, type CacheStats } from '../api/client'
import {
  ArchiveBoxIcon,
  DocumentIcon,
  Square3Stack3DIcon,
  ArrowTrendingUpIcon,
  ArrowPathIcon
} from '@heroicons/vue/24/outline'

const stats = ref<CacheStats | null>(null)
const loading = ref(true)
const error = ref('')

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

const hitRatePercent = computed(() => {
  if (!stats.value) return '0.0'
  return (stats.value.hit_rate * 100).toFixed(1)
})

onMounted(fetchStats)
</script>

<template>
  <div class="p-8">
    <div class="mb-8">
      <h1 class="text-2xl font-bold text-gray-900">Dashboard</h1>
      <p class="mt-1 text-sm text-gray-500">Overview of your Harbor Cache instance</p>
    </div>

    <!-- Error message -->
    <div v-if="error" class="mb-6 rounded-md bg-red-50 p-4">
      <p class="text-sm text-red-700">{{ error }}</p>
    </div>

    <!-- Loading state -->
    <div v-if="loading" class="flex items-center justify-center h-64">
      <ArrowPathIcon class="h-8 w-8 text-gray-400 animate-spin" />
    </div>

    <!-- Stats grid -->
    <div v-else-if="stats" class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
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

    <!-- Cache Performance -->
    <div v-if="stats" class="mt-8">
      <h2 class="text-lg font-semibold text-gray-900 mb-4">Cache Performance</h2>
      <div class="card">
        <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
          <!-- Hit Rate -->
          <div class="text-center">
            <div class="flex items-center justify-center mb-2">
              <ArrowTrendingUpIcon class="h-8 w-8 text-green-600" />
            </div>
            <p class="text-3xl font-bold text-gray-900">{{ hitRatePercent }}%</p>
            <p class="text-sm text-gray-500">Hit Rate</p>
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
    </div>

    <!-- Refresh button -->
    <div class="mt-6">
      <button
        @click="fetchStats"
        :disabled="loading"
        class="btn btn-secondary flex items-center gap-2"
      >
        <ArrowPathIcon class="h-4 w-4" :class="{ 'animate-spin': loading }" />
        Refresh
      </button>
    </div>
  </div>
</template>
