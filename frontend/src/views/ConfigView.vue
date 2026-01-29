<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { configApi, type ConfigSchema, type ConfigSchemaField } from '../api/client'
import {
  ArrowPathIcon,
  CheckIcon,
  ExclamationTriangleIcon,
  DocumentTextIcon,
  Cog6ToothIcon
} from '@heroicons/vue/24/outline'

const schema = ref<ConfigSchema | null>(null)
const configContent = ref('')
const originalContent = ref('')
const loading = ref(true)
const saving = ref(false)
const validating = ref(false)
const error = ref('')
const success = ref('')
const validationResult = ref<{ valid: boolean; message: string } | null>(null)
const activeTab = ref<'form' | 'raw'>('form')
const activeGroup = ref<string>('server')

// Form values - maps field key to value
const formValues = ref<Record<string, string>>({})

async function fetchSchema() {
  try {
    const response = await configApi.getSchema()
    schema.value = response.data
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Failed to fetch config schema'
  }
}

async function fetchConfigFile() {
  try {
    const response = await configApi.getFile()
    configContent.value = response.data.content
    originalContent.value = response.data.content
    parseConfigToForm(response.data.content)
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Failed to fetch config file'
  }
}

function parseConfigToForm(content: string) {
  // Simple TOML parsing for form values
  const lines = content.split('\n')
  let currentSection = ''
  let currentSubSection = ''

  for (const line of lines) {
    const trimmed = line.trim()

    // Section header
    const sectionMatch = trimmed.match(/^\[([^\]]+)\]$/)
    if (sectionMatch) {
      const parts = sectionMatch[1].split('.')
      currentSection = parts[0]
      currentSubSection = parts.length > 1 ? parts[1] : ''
      continue
    }

    // Key-value pair
    const kvMatch = trimmed.match(/^([a-z_]+)\s*=\s*(.+)$/)
    if (kvMatch) {
      let key = currentSection
      if (currentSubSection) {
        key += '.' + currentSubSection
      }
      key += '.' + kvMatch[1]

      let value = kvMatch[2].trim()
      // Remove quotes
      if ((value.startsWith('"') && value.endsWith('"')) ||
          (value.startsWith("'") && value.endsWith("'"))) {
        value = value.slice(1, -1)
      }

      formValues.value[key] = value
    }
  }
}

function generateConfigFromForm(): string {
  if (!schema.value) return configContent.value

  const groups: Record<string, ConfigSchemaField[]> = {}
  for (const field of schema.value.fields) {
    const parts = field.key.split('.')
    const groupKey = parts.length > 2 ? `${parts[0]}.${parts[1]}` : parts[0]
    if (!groups[groupKey]) {
      groups[groupKey] = []
    }
    groups[groupKey].push(field)
  }

  let result = '# Harbor Cache Configuration\n\n'

  const processedSections = new Set<string>()

  for (const [groupKey, fields] of Object.entries(groups)) {
    const parts = groupKey.split('.')
    const section = parts[0]
    const subSection = parts.length > 1 ? parts[1] : null

    if (subSection) {
      result += `[${section}.${subSection}]\n`
    } else if (!processedSections.has(section)) {
      result += `[${section}]\n`
      processedSections.add(section)
    }

    for (const field of fields) {
      const value = formValues.value[field.key]
      if (value !== undefined && value !== '') {
        const fieldName = field.key.split('.').pop()
        if (field.field_type === 'number') {
          result += `${fieldName} = ${value}\n`
        } else if (field.field_type === 'boolean') {
          result += `${fieldName} = ${value}\n`
        } else {
          result += `${fieldName} = "${value}"\n`
        }
      }
    }
    result += '\n'
  }

  return result.trim() + '\n'
}

async function fetchAll() {
  loading.value = true
  error.value = ''
  await Promise.all([fetchSchema(), fetchConfigFile()])
  loading.value = false
}

async function handleValidate() {
  validating.value = true
  validationResult.value = null
  error.value = ''

  const content = activeTab.value === 'form' ? generateConfigFromForm() : configContent.value

  try {
    const response = await configApi.validate(content)
    validationResult.value = response.data
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Validation failed'
  } finally {
    validating.value = false
  }
}

async function handleSave() {
  saving.value = true
  error.value = ''
  success.value = ''
  validationResult.value = null

  const content = activeTab.value === 'form' ? generateConfigFromForm() : configContent.value

  try {
    // First validate
    const validateResponse = await configApi.validate(content)
    if (!validateResponse.data.valid) {
      error.value = validateResponse.data.message
      return
    }

    // Then save
    const response = await configApi.updateFile(content)
    success.value = response.data.message
    originalContent.value = content
    configContent.value = content

    if (activeTab.value === 'raw') {
      parseConfigToForm(content)
    }
  } catch (err: any) {
    error.value = err.response?.data?.errors?.[0]?.message || 'Save failed'
  } finally {
    saving.value = false
  }
}

const hasChanges = computed(() => {
  if (activeTab.value === 'raw') {
    return configContent.value !== originalContent.value
  }
  const generated = generateConfigFromForm()
  return generated !== originalContent.value
})

const groupedFields = computed(() => {
  if (!schema.value) return {}
  const grouped: Record<string, ConfigSchemaField[]> = {}
  for (const field of schema.value.fields) {
    if (!grouped[field.group]) {
      grouped[field.group] = []
    }
    grouped[field.group].push(field)
  }
  return grouped
})

function formatBytes(bytes: number): string {
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let unitIndex = 0
  let value = bytes
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024
    unitIndex++
  }
  return `${value.toFixed(value >= 100 ? 0 : 1)} ${units[unitIndex]}`
}

onMounted(fetchAll)
</script>

<template>
  <div class="p-8">
    <div class="mb-8 flex items-center justify-between">
      <div>
        <h1 class="text-2xl font-bold text-gray-900">Configuration</h1>
        <p class="mt-1 text-sm text-gray-500">Manage server configuration settings</p>
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
    <div v-if="validationResult" class="mb-6 rounded-md p-4" :class="validationResult.valid ? 'bg-green-50' : 'bg-yellow-50'">
      <div class="flex items-center gap-2">
        <CheckIcon v-if="validationResult.valid" class="h-5 w-5 text-green-600" />
        <ExclamationTriangleIcon v-else class="h-5 w-5 text-yellow-600" />
        <p class="text-sm" :class="validationResult.valid ? 'text-green-700' : 'text-yellow-700'">
          {{ validationResult.message }}
        </p>
      </div>
    </div>

    <!-- Loading state -->
    <div v-if="loading" class="flex items-center justify-center h-64">
      <ArrowPathIcon class="h-8 w-8 text-gray-400 animate-spin" />
    </div>

    <template v-else-if="schema">
      <!-- Tab Navigation -->
      <div class="flex border-b border-gray-200 mb-6">
        <button
          @click="activeTab = 'form'"
          class="px-4 py-2 text-sm font-medium border-b-2 -mb-px flex items-center gap-2"
          :class="activeTab === 'form' ? 'border-primary-500 text-primary-600' : 'border-transparent text-gray-500 hover:text-gray-700'"
        >
          <Cog6ToothIcon class="h-4 w-4" />
          Form Editor
        </button>
        <button
          @click="activeTab = 'raw'"
          class="px-4 py-2 text-sm font-medium border-b-2 -mb-px flex items-center gap-2"
          :class="activeTab === 'raw' ? 'border-primary-500 text-primary-600' : 'border-transparent text-gray-500 hover:text-gray-700'"
        >
          <DocumentTextIcon class="h-4 w-4" />
          Raw TOML
        </button>
      </div>

      <!-- Form Editor -->
      <div v-if="activeTab === 'form'" class="flex gap-6">
        <!-- Group Navigation -->
        <div class="w-48 flex-shrink-0">
          <nav class="space-y-1">
            <button
              v-for="group in schema.groups"
              :key="group.id"
              @click="activeGroup = group.id"
              class="w-full text-left px-3 py-2 text-sm rounded-md transition-colors"
              :class="activeGroup === group.id ? 'bg-primary-100 text-primary-700 font-medium' : 'text-gray-600 hover:bg-gray-100'"
            >
              {{ group.label }}
            </button>
          </nav>
        </div>

        <!-- Form Fields -->
        <div class="flex-1">
          <div v-for="group in schema.groups" :key="group.id" v-show="activeGroup === group.id">
            <div class="card">
              <div class="mb-6">
                <h2 class="text-lg font-semibold text-gray-900">{{ group.label }}</h2>
                <p class="text-sm text-gray-500">{{ group.description }}</p>
              </div>

              <div class="space-y-6">
                <div v-for="field in groupedFields[group.id]" :key="field.key">
                  <label class="label">
                    {{ field.label }}
                    <span v-if="field.required" class="text-red-500">*</span>
                  </label>
                  <p class="text-xs text-gray-500 mb-1">{{ field.description }}</p>

                  <!-- Select field -->
                  <select
                    v-if="field.field_type === 'select' && field.options"
                    v-model="formValues[field.key]"
                    class="input"
                  >
                    <option value="">Select...</option>
                    <option v-for="opt in field.options" :key="opt.value" :value="opt.value">
                      {{ opt.label }}
                    </option>
                  </select>

                  <!-- Boolean field -->
                  <select
                    v-else-if="field.field_type === 'boolean'"
                    v-model="formValues[field.key]"
                    class="input"
                  >
                    <option value="true">Yes</option>
                    <option value="false">No</option>
                  </select>

                  <!-- Password field -->
                  <input
                    v-else-if="field.field_type === 'password'"
                    v-model="formValues[field.key]"
                    type="password"
                    class="input"
                    :placeholder="field.default_value || ''"
                  />

                  <!-- Number field -->
                  <div v-else-if="field.field_type === 'number'" class="relative">
                    <input
                      v-model="formValues[field.key]"
                      type="number"
                      class="input"
                      :placeholder="field.default_value || ''"
                    />
                    <span
                      v-if="field.key === 'cache.max_size' && formValues[field.key]"
                      class="absolute right-3 top-1/2 -translate-y-1/2 text-xs text-gray-500"
                    >
                      {{ formatBytes(Number(formValues[field.key])) }}
                    </span>
                  </div>

                  <!-- Text field -->
                  <input
                    v-else
                    v-model="formValues[field.key]"
                    type="text"
                    class="input"
                    :placeholder="field.default_value || ''"
                  />
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Raw TOML Editor -->
      <div v-else class="card">
        <div class="mb-4">
          <h2 class="text-lg font-semibold text-gray-900">Raw Configuration (TOML)</h2>
          <p class="text-sm text-gray-500">Edit the configuration file directly</p>
        </div>
        <textarea
          v-model="configContent"
          class="w-full h-96 font-mono text-sm p-4 border border-gray-300 rounded-md focus:ring-primary-500 focus:border-primary-500"
          placeholder="# Configuration content..."
        ></textarea>
      </div>

      <!-- Action Buttons -->
      <div class="mt-6 flex items-center justify-between">
        <div class="flex items-center gap-2">
          <span v-if="hasChanges" class="text-sm text-yellow-600 flex items-center gap-1">
            <ExclamationTriangleIcon class="h-4 w-4" />
            Unsaved changes
          </span>
        </div>
        <div class="flex gap-3">
          <button
            @click="handleValidate"
            :disabled="validating"
            class="btn btn-secondary"
          >
            {{ validating ? 'Validating...' : 'Validate' }}
          </button>
          <button
            @click="handleSave"
            :disabled="saving || !hasChanges"
            class="btn btn-primary"
          >
            {{ saving ? 'Saving...' : 'Save Changes' }}
          </button>
        </div>
      </div>

      <!-- Restart Notice -->
      <div class="mt-6 rounded-md bg-blue-50 p-4">
        <div class="flex items-start gap-3">
          <ExclamationTriangleIcon class="h-5 w-5 text-blue-600 flex-shrink-0 mt-0.5" />
          <div>
            <p class="text-sm font-medium text-blue-800">Server Restart Required</p>
            <p class="text-sm text-blue-700 mt-1">
              Most configuration changes require a server restart to take effect.
              After saving, restart the Harbor Cache server to apply the new settings.
            </p>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>
