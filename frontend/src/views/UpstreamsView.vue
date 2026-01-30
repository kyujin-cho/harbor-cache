<script setup lang="ts">
import { ref, onMounted } from 'vue'
import {
  upstreamsApi,
  type Upstream,
  type UpstreamHealth,
  type CreateUpstreamRequest,
  type UpdateUpstreamRequest
} from '../api/client'
import {
  PlusIcon,
  ArrowPathIcon,
  PencilIcon,
  TrashIcon,
  CheckCircleIcon,
  XCircleIcon,
  ServerIcon,
  LinkIcon,
  BeakerIcon
} from '@heroicons/vue/24/outline'

const upstreams = ref<Upstream[]>([])
const healthStatus = ref<Map<string, UpstreamHealth>>(new Map())
const loading = ref(true)
const error = ref('')
const configPath = ref<string | null>(null)

// Modal state
const showModal = ref(false)
const modalMode = ref<'create' | 'edit'>('create')
const editingUpstream = ref<Upstream | null>(null)

// Form state
const formData = ref<CreateUpstreamRequest>({
  name: '',
  display_name: '',
  url: '',
  registry: '',
  username: '',
  password: '',
  skip_tls_verify: false,
  priority: 100,
  enabled: true,
  cache_isolation: 'shared',
  is_default: false,
  routes: []
})

// Test connection state
const testing = ref(false)
const testResult = ref<{ success: boolean; message: string } | null>(null)

async function fetchUpstreams() {
  try {
    loading.value = true
    error.value = ''
    const response = await upstreamsApi.list()
    upstreams.value = response.data

    // Fetch health status for all upstreams
    try {
      const healthResponse = await upstreamsApi.getAllHealth()
      healthStatus.value = new Map(healthResponse.data.map(h => [h.name, h]))
    } catch (err) {
      console.error('Failed to fetch health status:', err)
    }

    // Fetch config path info
    try {
      const configResponse = await upstreamsApi.getConfigPath()
      configPath.value = configResponse.data.path
    } catch (err) {
      console.error('Failed to fetch config path:', err)
    }
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Failed to fetch upstreams'
  } finally {
    loading.value = false
  }
}

function openCreateModal() {
  modalMode.value = 'create'
  editingUpstream.value = null
  formData.value = {
    name: '',
    display_name: '',
    url: '',
    registry: '',
    username: '',
    password: '',
    skip_tls_verify: false,
    priority: 100,
    enabled: true,
    cache_isolation: 'shared',
    is_default: false,
    routes: []
  }
  testResult.value = null
  showModal.value = true
}

function openEditModal(upstream: Upstream) {
  modalMode.value = 'edit'
  editingUpstream.value = upstream
  formData.value = {
    name: upstream.name,
    display_name: upstream.display_name,
    url: upstream.url,
    registry: upstream.registry,
    username: '',
    password: '',
    skip_tls_verify: upstream.skip_tls_verify,
    priority: upstream.priority,
    enabled: upstream.enabled,
    cache_isolation: upstream.cache_isolation,
    is_default: upstream.is_default,
    routes: []
  }
  testResult.value = null
  showModal.value = true
}

function closeModal() {
  showModal.value = false
  editingUpstream.value = null
  testResult.value = null
}

async function testConnection() {
  testing.value = true
  testResult.value = null
  try {
    const response = await upstreamsApi.test({
      url: formData.value.url,
      registry: formData.value.registry,
      username: formData.value.username || undefined,
      password: formData.value.password || undefined,
      skip_tls_verify: formData.value.skip_tls_verify
    })
    testResult.value = response.data
  } catch (err: any) {
    testResult.value = {
      success: false,
      message: err.response?.data?.errors?.[0]?.message || 'Test failed'
    }
  } finally {
    testing.value = false
  }
}

async function saveUpstream() {
  try {
    if (modalMode.value === 'create') {
      await upstreamsApi.create(formData.value)
    } else if (editingUpstream.value) {
      const update: UpdateUpstreamRequest = {
        display_name: formData.value.display_name,
        url: formData.value.url,
        registry: formData.value.registry,
        skip_tls_verify: formData.value.skip_tls_verify,
        priority: formData.value.priority,
        enabled: formData.value.enabled,
        cache_isolation: formData.value.cache_isolation,
        is_default: formData.value.is_default
      }
      // Only send credentials if provided
      if (formData.value.username) {
        update.username = formData.value.username
      }
      if (formData.value.password) {
        update.password = formData.value.password
      }
      // Use name instead of id
      await upstreamsApi.update(editingUpstream.value.name, update)
    }
    closeModal()
    fetchUpstreams()
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Failed to save upstream'
  }
}

async function deleteUpstream(upstream: Upstream) {
  if (!confirm(`Are you sure you want to delete upstream "${upstream.display_name}"? This will update the config file.`)) {
    return
  }
  try {
    // Use name instead of id
    await upstreamsApi.delete(upstream.name)
    fetchUpstreams()
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Failed to delete upstream'
  }
}

function getHealthClass(upstreamName: string): string {
  const health = healthStatus.value.get(upstreamName)
  if (!health) return 'text-gray-400'
  return health.healthy ? 'text-green-500' : 'text-red-500'
}

onMounted(fetchUpstreams)
</script>

<template>
  <div class="p-8">
    <div class="mb-8 flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold text-gray-900">Upstreams</h1>
        <p class="mt-1 text-sm text-gray-500">Manage upstream Harbor registries</p>
        <p v-if="configPath" class="mt-1 text-xs text-blue-600">
          Changes are persisted to: <code class="font-mono bg-blue-50 px-1 rounded">{{ configPath }}</code>
        </p>
      </div>
      <div class="flex items-center gap-3">
        <button
          @click="fetchUpstreams"
          :disabled="loading"
          class="btn btn-secondary flex items-center gap-2"
        >
          <ArrowPathIcon class="h-4 w-4" :class="{ 'animate-spin': loading }" />
          Refresh
        </button>
        <button
          @click="openCreateModal"
          class="btn btn-primary flex items-center gap-2"
        >
          <PlusIcon class="h-4 w-4" />
          Add Upstream
        </button>
      </div>
    </div>

    <!-- Error message -->
    <div v-if="error" class="mb-6 rounded-md bg-red-50 p-4">
      <p class="text-sm text-red-700">{{ error }}</p>
    </div>

    <!-- Loading state -->
    <div v-if="loading && upstreams.length === 0" class="flex items-center justify-center h-64">
      <ArrowPathIcon class="h-8 w-8 text-gray-400 animate-spin" />
    </div>

    <!-- Empty state -->
    <div v-else-if="upstreams.length === 0" class="text-center py-12">
      <ServerIcon class="mx-auto h-12 w-12 text-gray-400" />
      <h3 class="mt-2 text-sm font-semibold text-gray-900">No upstreams</h3>
      <p class="mt-1 text-sm text-gray-500">Get started by adding an upstream Harbor registry.</p>
      <div class="mt-6">
        <button @click="openCreateModal" class="btn btn-primary">
          <PlusIcon class="h-4 w-4 mr-2" />
          Add Upstream
        </button>
      </div>
    </div>

    <!-- Upstreams list -->
    <div v-else class="grid gap-6 md:grid-cols-2 lg:grid-cols-3">
      <div
        v-for="upstream in upstreams"
        :key="upstream.id"
        class="card hover:shadow-lg transition-shadow"
      >
        <div class="flex items-start justify-between mb-4">
          <div class="flex items-center gap-3">
            <div class="p-2 rounded-lg" :class="upstream.enabled ? 'bg-primary-100' : 'bg-gray-100'">
              <ServerIcon class="h-6 w-6" :class="upstream.enabled ? 'text-primary-600' : 'text-gray-400'" />
            </div>
            <div>
              <h3 class="font-semibold text-gray-900">{{ upstream.display_name }}</h3>
              <p class="text-xs text-gray-500 font-mono">{{ upstream.name }}</p>
            </div>
          </div>
          <component
            :is="healthStatus.get(upstream.name)?.healthy ? CheckCircleIcon : XCircleIcon"
            class="h-5 w-5"
            :class="getHealthClass(upstream.name)"
          />
        </div>

        <div class="space-y-2 text-sm">
          <div class="flex items-center gap-2 text-gray-600">
            <LinkIcon class="h-4 w-4" />
            <span class="truncate">{{ upstream.url }}</span>
          </div>
          <div v-if="upstream.uses_multi_project" class="space-y-1">
            <div class="flex items-center gap-2">
              <span class="text-gray-500">Projects:</span>
              <span class="text-xs text-blue-600 font-medium">Multi-project mode</span>
            </div>
            <div class="flex flex-wrap gap-1">
              <span
                v-for="project in upstream.projects"
                :key="project.name"
                class="px-1.5 py-0.5 text-xs rounded bg-gray-100 text-gray-700 font-mono"
                :class="{ 'bg-blue-100 text-blue-700': project.is_default }"
                :title="project.effective_pattern"
              >
                {{ project.name }}
              </span>
            </div>
          </div>
          <div v-else class="flex items-center gap-2">
            <span class="text-gray-500">Registry:</span>
            <span class="font-mono text-gray-700">{{ upstream.registry }}</span>
          </div>
          <div class="flex items-center gap-2">
            <span class="text-gray-500">Priority:</span>
            <span class="text-gray-700">{{ upstream.priority }}</span>
          </div>
          <div class="flex flex-wrap gap-2 mt-3">
            <span
              v-if="upstream.is_default"
              class="px-2 py-0.5 text-xs rounded-full bg-yellow-100 text-yellow-700"
            >
              Default
            </span>
            <span
              :class="upstream.enabled
                ? 'bg-green-100 text-green-700'
                : 'bg-gray-100 text-gray-500'"
              class="px-2 py-0.5 text-xs rounded-full"
            >
              {{ upstream.enabled ? 'Enabled' : 'Disabled' }}
            </span>
            <span class="px-2 py-0.5 text-xs rounded-full bg-blue-100 text-blue-700">
              {{ upstream.cache_isolation }}
            </span>
            <span
              v-if="upstream.has_credentials"
              class="px-2 py-0.5 text-xs rounded-full bg-purple-100 text-purple-700"
            >
              Auth
            </span>
          </div>
        </div>

        <div class="mt-4 pt-4 border-t flex justify-end gap-2">
          <button
            @click="openEditModal(upstream)"
            class="btn btn-secondary btn-sm flex items-center gap-1"
          >
            <PencilIcon class="h-4 w-4" />
            Edit
          </button>
          <button
            @click="deleteUpstream(upstream)"
            class="btn btn-danger btn-sm flex items-center gap-1"
          >
            <TrashIcon class="h-4 w-4" />
            Delete
          </button>
        </div>
      </div>
    </div>

    <!-- Modal -->
    <Teleport to="body">
      <div v-if="showModal" class="fixed inset-0 z-50 overflow-y-auto">
        <div class="flex min-h-screen items-center justify-center p-4">
          <div class="fixed inset-0 bg-black/50" @click="closeModal"></div>
          <div class="relative bg-white rounded-lg shadow-xl max-w-lg w-full p-6">
            <h2 class="text-lg font-semibold mb-4">
              {{ modalMode === 'create' ? 'Add Upstream' : 'Edit Upstream' }}
            </h2>

            <form @submit.prevent="saveUpstream" class="space-y-4">
              <div v-if="modalMode === 'create'">
                <label class="block text-sm font-medium text-gray-700">Name</label>
                <input
                  v-model="formData.name"
                  type="text"
                  required
                  class="input mt-1"
                  placeholder="my-harbor"
                />
                <p class="mt-1 text-xs text-gray-500">Unique identifier (alphanumeric, dashes, underscores)</p>
              </div>

              <div>
                <label class="block text-sm font-medium text-gray-700">Display Name</label>
                <input
                  v-model="formData.display_name"
                  type="text"
                  required
                  class="input mt-1"
                  placeholder="My Harbor Registry"
                />
              </div>

              <div>
                <label class="block text-sm font-medium text-gray-700">URL</label>
                <input
                  v-model="formData.url"
                  type="url"
                  required
                  class="input mt-1"
                  placeholder="https://harbor.example.com"
                />
              </div>

              <div>
                <label class="block text-sm font-medium text-gray-700">Registry/Project</label>
                <input
                  v-model="formData.registry"
                  type="text"
                  required
                  class="input mt-1"
                  placeholder="library"
                />
              </div>

              <div class="grid grid-cols-2 gap-4">
                <div>
                  <label class="block text-sm font-medium text-gray-700">Username</label>
                  <input
                    v-model="formData.username"
                    type="text"
                    class="input mt-1"
                    :placeholder="modalMode === 'edit' ? '(unchanged)' : 'Optional'"
                  />
                </div>
                <div>
                  <label class="block text-sm font-medium text-gray-700">Password</label>
                  <input
                    v-model="formData.password"
                    type="password"
                    class="input mt-1"
                    :placeholder="modalMode === 'edit' ? '(unchanged)' : 'Optional'"
                  />
                </div>
              </div>

              <div class="grid grid-cols-2 gap-4">
                <div>
                  <label class="block text-sm font-medium text-gray-700">Priority</label>
                  <input
                    v-model.number="formData.priority"
                    type="number"
                    class="input mt-1"
                    min="0"
                  />
                  <p class="mt-1 text-xs text-gray-500">Lower = higher priority</p>
                </div>
                <div>
                  <label class="block text-sm font-medium text-gray-700">Cache Isolation</label>
                  <select v-model="formData.cache_isolation" class="input mt-1">
                    <option value="shared">Shared</option>
                    <option value="isolated">Isolated</option>
                  </select>
                </div>
              </div>

              <div class="flex flex-wrap gap-4">
                <label class="flex items-center gap-2">
                  <input v-model="formData.enabled" type="checkbox" class="rounded" />
                  <span class="text-sm text-gray-700">Enabled</span>
                </label>
                <label class="flex items-center gap-2">
                  <input v-model="formData.is_default" type="checkbox" class="rounded" />
                  <span class="text-sm text-gray-700">Default upstream</span>
                </label>
                <label class="flex items-center gap-2">
                  <input v-model="formData.skip_tls_verify" type="checkbox" class="rounded" />
                  <span class="text-sm text-gray-700">Skip TLS verify</span>
                </label>
              </div>

              <!-- Test connection -->
              <div class="pt-4 border-t">
                <button
                  type="button"
                  @click="testConnection"
                  :disabled="testing || !formData.url || !formData.registry"
                  class="btn btn-secondary flex items-center gap-2"
                >
                  <BeakerIcon class="h-4 w-4" />
                  {{ testing ? 'Testing...' : 'Test Connection' }}
                </button>
                <div
                  v-if="testResult"
                  class="mt-2 p-2 rounded text-sm"
                  :class="testResult.success ? 'bg-green-50 text-green-700' : 'bg-red-50 text-red-700'"
                >
                  {{ testResult.message }}
                </div>
              </div>

              <div class="flex justify-end gap-3 pt-4">
                <button type="button" @click="closeModal" class="btn btn-secondary">
                  Cancel
                </button>
                <button type="submit" class="btn btn-primary">
                  {{ modalMode === 'create' ? 'Create' : 'Save' }}
                </button>
              </div>
            </form>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>
