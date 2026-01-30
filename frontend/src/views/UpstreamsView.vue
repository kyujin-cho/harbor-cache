<script setup lang="ts">
import { ref, onMounted } from 'vue'
import {
  upstreamsApi,
  type Upstream,
  type UpstreamHealth,
  type UpstreamProject,
  type CreateUpstreamRequest,
  type UpdateUpstreamRequest,
  type UpdateUpstreamProjectRequest
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
  BeakerIcon,
  ChevronDownIcon,
  ChevronRightIcon,
  FolderIcon,
  ExclamationTriangleIcon
} from '@heroicons/vue/24/outline'

const upstreams = ref<Upstream[]>([])
const healthStatus = ref<Map<string, UpstreamHealth>>(new Map())
const loading = ref(true)
const error = ref('')
const configPath = ref<string | null>(null)

// Track expanded upstreams for project list
const expandedUpstreams = ref<Set<string>>(new Set())

// Modal state for upstream
const showModal = ref(false)
const modalMode = ref<'create' | 'edit'>('create')
const editingUpstream = ref<Upstream | null>(null)

// Modal state for project
const showProjectModal = ref(false)
const projectModalMode = ref<'create' | 'edit'>('create')
const editingProjectUpstream = ref<Upstream | null>(null)
const editingProjectIndex = ref<number>(-1)
const projectFormErrors = ref<{ name?: string; pattern?: string }>({})

// Form state for upstream
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

// Form state for project
const projectFormData = ref<UpdateUpstreamProjectRequest>({
  name: '',
  pattern: null,
  priority: 100,
  is_default: false
})

// Test connection state
const testing = ref(false)
const testResult = ref<{ success: boolean; message: string } | null>(null)

// Saving state for project operations
const savingProject = ref(false)

// ==================== Validation Functions ====================

function validateProjectName(name: string): string | null {
  if (!name || name.trim() === '') {
    return 'Project name is required'
  }
  if (name.length > 256) {
    return 'Project name exceeds maximum length of 256 characters'
  }
  if (name.includes('..')) {
    return 'Project name cannot contain path traversal sequences (..)'
  }
  if (!/^[a-zA-Z0-9]/.test(name)) {
    return 'Project name must start with an alphanumeric character'
  }
  if (!/^[a-zA-Z0-9][a-zA-Z0-9._/-]*$/.test(name)) {
    return 'Project name must contain only alphanumeric characters, dashes, underscores, dots, and forward slashes'
  }
  return null
}

function validatePattern(pattern: string): string | null {
  if (!pattern) {
    return null // Pattern is optional
  }
  if (pattern.length > 512) {
    return 'Pattern exceeds maximum length of 512 characters'
  }
  if (pattern.includes('..')) {
    return 'Pattern cannot contain path traversal sequences (..)'
  }
  const wildcardCount = (pattern.match(/\*/g) || []).length
  if (wildcardCount > 10) {
    return `Pattern contains ${wildcardCount} wildcards, maximum allowed is 10`
  }
  return null
}

function validateProjectForm(): boolean {
  projectFormErrors.value = {}

  const nameError = validateProjectName(projectFormData.value.name)
  if (nameError) {
    projectFormErrors.value.name = nameError
  }

  if (projectFormData.value.pattern) {
    const patternError = validatePattern(projectFormData.value.pattern)
    if (patternError) {
      projectFormErrors.value.pattern = patternError
    }
  }

  // Check for duplicate project names within the upstream
  if (editingProjectUpstream.value && !projectFormErrors.value.name) {
    const existingProjects = editingProjectUpstream.value.projects || []
    const isDuplicate = existingProjects.some((p, idx) =>
      p.name === projectFormData.value.name &&
      (projectModalMode.value === 'create' || idx !== editingProjectIndex.value)
    )
    if (isDuplicate) {
      projectFormErrors.value.name = `A project named '${projectFormData.value.name}' already exists in this upstream`
    }
  }

  return Object.keys(projectFormErrors.value).length === 0
}

// ==================== Data Fetching ====================

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

// ==================== Upstream Modal Functions ====================

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

// ==================== Project Modal Functions ====================

function toggleUpstreamExpand(upstreamName: string) {
  if (expandedUpstreams.value.has(upstreamName)) {
    expandedUpstreams.value.delete(upstreamName)
  } else {
    expandedUpstreams.value.add(upstreamName)
  }
}

function openAddProjectModal(upstream: Upstream) {
  projectModalMode.value = 'create'
  editingProjectUpstream.value = upstream
  editingProjectIndex.value = -1
  projectFormData.value = {
    name: '',
    pattern: null,
    priority: 100,
    is_default: false
  }
  projectFormErrors.value = {}
  showProjectModal.value = true
}

function openEditProjectModal(upstream: Upstream, project: UpstreamProject, index: number) {
  projectModalMode.value = 'edit'
  editingProjectUpstream.value = upstream
  editingProjectIndex.value = index
  projectFormData.value = {
    name: project.name,
    pattern: project.pattern || null,
    priority: project.priority,
    is_default: project.is_default
  }
  projectFormErrors.value = {}
  showProjectModal.value = true
}

function closeProjectModal() {
  showProjectModal.value = false
  editingProjectUpstream.value = null
  editingProjectIndex.value = -1
  projectFormErrors.value = {}
}

// ==================== API Operations ====================

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

async function saveProject() {
  if (!validateProjectForm()) {
    return
  }

  if (!editingProjectUpstream.value) {
    return
  }

  savingProject.value = true
  error.value = ''

  try {
    // Build the updated projects array - map to UpdateUpstreamProjectRequest format
    const existingProjects: UpdateUpstreamProjectRequest[] = (editingProjectUpstream.value.projects || []).map(p => ({
      name: p.name,
      pattern: p.pattern,
      priority: p.priority,
      is_default: p.is_default
    }))

    const newProject: UpdateUpstreamProjectRequest = {
      name: projectFormData.value.name.trim(),
      pattern: projectFormData.value.pattern?.trim() || null,
      priority: projectFormData.value.priority,
      is_default: projectFormData.value.is_default
    }

    if (projectModalMode.value === 'create') {
      existingProjects.push(newProject)
    } else {
      existingProjects[editingProjectIndex.value] = newProject
    }

    // If this project is set as default, unset other defaults
    if (newProject.is_default) {
      existingProjects.forEach((p, idx) => {
        if (p.name !== newProject.name) {
          existingProjects[idx] = { ...p, is_default: false }
        }
      })
    }

    // Send update to API
    const upstreamName = editingProjectUpstream.value.name
    await upstreamsApi.update(upstreamName, {
      projects: existingProjects
    })

    closeProjectModal()
    await fetchUpstreams()

    // Keep the upstream expanded
    expandedUpstreams.value.add(upstreamName)
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Failed to save project'
  } finally {
    savingProject.value = false
  }
}

async function deleteProject(upstream: Upstream, projectIndex: number) {
  const project = upstream.projects[projectIndex]
  if (!confirm(`Are you sure you want to delete project "${project.name}" from upstream "${upstream.display_name}"?`)) {
    return
  }

  error.value = ''

  try {
    // Build the updated projects array without the deleted project
    const updatedProjects = upstream.projects
      .filter((_, idx) => idx !== projectIndex)
      .map(p => ({
        name: p.name,
        pattern: p.pattern,
        priority: p.priority,
        is_default: p.is_default
      }))

    // Send update to API
    await upstreamsApi.update(upstream.name, {
      projects: updatedProjects
    })

    await fetchUpstreams()

    // Keep the upstream expanded
    expandedUpstreams.value.add(upstream.name)
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Failed to delete project'
  }
}

// ==================== Helper Functions ====================

function getHealthClass(upstreamName: string): string {
  const health = healthStatus.value.get(upstreamName)
  if (!health) return 'text-gray-400'
  return health.healthy ? 'text-green-500' : 'text-red-500'
}

function getProjectCount(upstream: Upstream): number {
  return upstream.projects?.length || 0
}

function hasNoDefaultProject(upstream: Upstream): boolean {
  if (!upstream.uses_multi_project || !upstream.projects?.length) {
    return false
  }
  return !upstream.projects.some(p => p.is_default)
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
    <div v-else class="space-y-6">
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

          <!-- Multi-project mode: Collapsible project list -->
          <div v-if="upstream.uses_multi_project" class="mt-3">
            <button
              @click="toggleUpstreamExpand(upstream.name)"
              class="flex items-center gap-2 text-sm font-medium text-gray-700 hover:text-gray-900"
            >
              <component
                :is="expandedUpstreams.has(upstream.name) ? ChevronDownIcon : ChevronRightIcon"
                class="h-4 w-4"
              />
              <FolderIcon class="h-4 w-4 text-blue-500" />
              <span>Projects ({{ getProjectCount(upstream) }})</span>
              <span class="text-xs text-blue-600 font-medium">Multi-project mode</span>
              <!-- Warning if no default project -->
              <span
                v-if="hasNoDefaultProject(upstream)"
                class="flex items-center gap-1 text-xs text-amber-600"
                title="No default project set - requests not matching any pattern may fail"
              >
                <ExclamationTriangleIcon class="h-4 w-4" />
                No default
              </span>
            </button>

            <!-- Expanded project list -->
            <div v-if="expandedUpstreams.has(upstream.name)" class="mt-2 ml-6 space-y-2">
              <div
                v-for="(project, idx) in upstream.projects"
                :key="project.name"
                class="flex items-center justify-between p-2 bg-gray-50 rounded-md"
              >
                <div class="flex items-center gap-3">
                  <div class="min-w-0">
                    <div class="flex items-center gap-2">
                      <span class="font-mono text-sm text-gray-800">{{ project.name }}</span>
                      <span
                        v-if="project.is_default"
                        class="px-1.5 py-0.5 text-xs rounded bg-blue-100 text-blue-700"
                      >
                        Default
                      </span>
                    </div>
                    <div class="text-xs text-gray-500">
                      <span class="font-medium">Pattern:</span>
                      <code class="ml-1 bg-gray-100 px-1 rounded">{{ project.effective_pattern }}</code>
                      <span class="ml-2 font-medium">Priority:</span>
                      <span class="ml-1">{{ project.priority }}</span>
                    </div>
                  </div>
                </div>
                <div class="flex items-center gap-1">
                  <button
                    @click="openEditProjectModal(upstream, project, idx)"
                    class="p-1 text-gray-400 hover:text-gray-600"
                    title="Edit project"
                  >
                    <PencilIcon class="h-4 w-4" />
                  </button>
                  <button
                    @click="deleteProject(upstream, idx)"
                    class="p-1 text-gray-400 hover:text-red-600"
                    title="Delete project"
                  >
                    <TrashIcon class="h-4 w-4" />
                  </button>
                </div>
              </div>

              <!-- Add project button -->
              <button
                @click="openAddProjectModal(upstream)"
                class="flex items-center gap-2 px-3 py-2 text-sm text-blue-600 hover:text-blue-800 hover:bg-blue-50 rounded-md w-full"
              >
                <PlusIcon class="h-4 w-4" />
                Add Project
              </button>
            </div>
          </div>

          <!-- Single-project mode: Show registry -->
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

    <!-- Upstream Modal -->
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

    <!-- Project Modal -->
    <Teleport to="body">
      <div v-if="showProjectModal" class="fixed inset-0 z-50 overflow-y-auto">
        <div class="flex min-h-screen items-center justify-center p-4">
          <div class="fixed inset-0 bg-black/50" @click="closeProjectModal"></div>
          <div class="relative bg-white rounded-lg shadow-xl max-w-md w-full p-6">
            <h2 class="text-lg font-semibold mb-4">
              {{ projectModalMode === 'create' ? 'Add Project' : 'Edit Project' }}
            </h2>
            <p v-if="editingProjectUpstream" class="text-sm text-gray-500 mb-4">
              Upstream: <span class="font-medium">{{ editingProjectUpstream.display_name }}</span>
            </p>

            <form @submit.prevent="saveProject" class="space-y-4">
              <div>
                <label class="block text-sm font-medium text-gray-700">
                  Project Name <span class="text-red-500">*</span>
                </label>
                <input
                  v-model="projectFormData.name"
                  type="text"
                  required
                  class="input mt-1"
                  :class="{ 'border-red-500': projectFormErrors.name }"
                  placeholder="library"
                />
                <p v-if="projectFormErrors.name" class="mt-1 text-xs text-red-600">
                  {{ projectFormErrors.name }}
                </p>
                <p v-else class="mt-1 text-xs text-gray-500">
                  The project/registry name in Harbor (e.g., library, team-a)
                </p>
              </div>

              <div>
                <label class="block text-sm font-medium text-gray-700">Pattern</label>
                <input
                  v-model="projectFormData.pattern"
                  type="text"
                  class="input mt-1"
                  :class="{ 'border-red-500': projectFormErrors.pattern }"
                  placeholder="library/* or team-a/**"
                />
                <p v-if="projectFormErrors.pattern" class="mt-1 text-xs text-red-600">
                  {{ projectFormErrors.pattern }}
                </p>
                <p v-else class="mt-1 text-xs text-gray-500">
                  Glob pattern to match repository paths. Use * for single segment, ** for multiple. Default: {name}/*
                </p>
              </div>

              <div>
                <label class="block text-sm font-medium text-gray-700">Priority</label>
                <input
                  v-model.number="projectFormData.priority"
                  type="number"
                  class="input mt-1"
                  min="0"
                />
                <p class="mt-1 text-xs text-gray-500">
                  Lower number = higher priority. Used when multiple patterns match.
                </p>
              </div>

              <div>
                <label class="flex items-center gap-2">
                  <input v-model="projectFormData.is_default" type="checkbox" class="rounded" />
                  <span class="text-sm text-gray-700">Default project</span>
                </label>
                <p class="mt-1 text-xs text-gray-500 ml-6">
                  Used when no other project patterns match the request
                </p>
              </div>

              <div class="flex justify-end gap-3 pt-4 border-t">
                <button type="button" @click="closeProjectModal" class="btn btn-secondary">
                  Cancel
                </button>
                <button
                  type="submit"
                  class="btn btn-primary"
                  :disabled="savingProject"
                >
                  {{ savingProject ? 'Saving...' : (projectModalMode === 'create' ? 'Add Project' : 'Save Changes') }}
                </button>
              </div>
            </form>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>
