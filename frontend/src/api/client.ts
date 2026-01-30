import axios from 'axios'

const api = axios.create({
  baseURL: '/api/v1',
  headers: {
    'Content-Type': 'application/json'
  }
})

// Request interceptor to add auth token
api.interceptors.request.use((config) => {
  const token = localStorage.getItem('token')
  if (token) {
    config.headers.Authorization = `Bearer ${token}`
  }
  return config
})

// Response interceptor to handle auth errors
api.interceptors.response.use(
  (response) => response,
  (error) => {
    if (error.response?.status === 401) {
      localStorage.removeItem('token')
      localStorage.removeItem('user')
      window.location.href = '/login'
    }
    return Promise.reject(error)
  }
)

// Auth API
export const authApi = {
  login: (username: string, password: string) =>
    api.post<{ token: string; expires_in: number }>('/auth/login', { username, password })
}

// Cache API
export interface CacheStats {
  total_size: number
  total_size_human: string
  entry_count: number
  manifest_count: number
  blob_count: number
  hit_count: number
  miss_count: number
  hit_rate: number
}

export interface CacheEntry {
  id: number
  entry_type: string
  repository: string | null
  reference: string | null
  digest: string
  content_type: string
  size: number
  size_human: string
  created_at: string
  last_accessed_at: string
  access_count: number
}

export interface CacheEntriesResponse {
  entries: CacheEntry[]
  total: number
  offset: number
  limit: number
}

export interface CacheEntriesQuery {
  entry_type?: string
  repository?: string
  digest?: string
  offset?: number
  limit?: number
  sort_by?: string
  sort_order?: string
}

export const cacheApi = {
  getStats: () => api.get<CacheStats>('/cache/stats'),
  getEntries: (query?: CacheEntriesQuery) => api.get<CacheEntriesResponse>('/cache/entries', { params: query }),
  getTopAccessed: () => api.get<CacheEntry[]>('/cache/entries/top'),
  getRepositories: () => api.get<{ repositories: string[] }>('/cache/repositories'),
  deleteEntry: (digest: string) => api.delete(`/cache/entries/${encodeURIComponent(digest)}`),
  clear: () => api.delete<{ cleared: number }>('/cache'),
  cleanup: () => api.post<{ cleaned: number }>('/cache/cleanup')
}

// Users API
export interface User {
  id: number
  username: string
  role: string
  created_at: string
  updated_at: string
}

export interface CreateUserRequest {
  username: string
  password: string
  role: string
}

export interface UpdateUserRequest {
  role?: string
  password?: string
}

export const usersApi = {
  list: () => api.get<User[]>('/users'),
  get: (id: number) => api.get<User>(`/users/${id}`),
  create: (data: CreateUserRequest) => api.post<User>('/users', data),
  update: (id: number, data: UpdateUserRequest) => api.put<User>(`/users/${id}`, data),
  delete: (id: number) => api.delete(`/users/${id}`)
}

// Config API
export interface ConfigEntry {
  key: string
  value: string
  updated_at: string
}

export interface UpdateConfigRequest {
  entries: Array<{ key: string; value: string }>
}

export interface ConfigOption {
  value: string
  label: string
}

export interface ConfigSchemaField {
  key: string
  label: string
  description: string
  field_type: string
  default_value: string | null
  required: boolean
  options: ConfigOption[] | null
  group: string
}

export interface ConfigGroup {
  id: string
  label: string
  description: string
}

export interface ConfigSchema {
  fields: ConfigSchemaField[]
  groups: ConfigGroup[]
}

export interface ConfigFileResponse {
  content: string
  format: string
}

export const configApi = {
  list: () => api.get<ConfigEntry[]>('/config'),
  get: (key: string) => api.get<ConfigEntry>(`/config/${key}`),
  update: (data: UpdateConfigRequest) => api.put<{ updated: number }>('/config', data),
  delete: (key: string) => api.delete(`/config/${key}`),
  getSchema: () => api.get<ConfigSchema>('/config/schema'),
  getFile: () => api.get<ConfigFileResponse>('/config/file'),
  updateFile: (content: string) => api.put<{ success: boolean; message: string }>('/config/file', { content }),
  validate: (content: string) => api.post<{ valid: boolean; message: string }>('/config/validate', { content })
}

// Activity Logs API
export interface ActivityLog {
  id: number
  timestamp: string
  action: string
  resource_type: string
  resource_id: string | null
  user_id: number | null
  username: string | null
  details: string | null
  ip_address: string | null
}

export interface ActivityLogsResponse {
  logs: ActivityLog[]
  total: number
  offset: number
  limit: number
}

export interface ActivityLogsQuery {
  action?: string
  resource_type?: string
  user_id?: number
  start_date?: string
  end_date?: string
  offset?: number
  limit?: number
}

export const logsApi = {
  list: (query?: ActivityLogsQuery) => api.get<ActivityLogsResponse>('/logs', { params: query }),
  getActions: () => api.get<string[]>('/logs/actions'),
  getResourceTypes: () => api.get<string[]>('/logs/resource-types')
}

// Upstreams API
export interface UpstreamProject {
  name: string
  pattern: string | null
  effective_pattern: string
  priority: number
  is_default: boolean
}

export interface Upstream {
  id: number
  name: string
  display_name: string
  url: string
  registry: string
  projects: UpstreamProject[]
  uses_multi_project: boolean
  skip_tls_verify: boolean
  priority: number
  enabled: boolean
  cache_isolation: string
  is_default: boolean
  has_credentials: boolean
  created_at: string
  updated_at: string
}

export interface UpstreamRoute {
  id: number
  upstream_id: number
  pattern: string
  priority: number
  created_at: string
}

export interface UpstreamHealth {
  upstream_id: number
  name: string
  healthy: boolean
  last_check: string
  last_error: string | null
  consecutive_failures: number
}

export interface CreateUpstreamRequest {
  name: string
  display_name: string
  url: string
  registry: string
  username?: string
  password?: string
  skip_tls_verify?: boolean
  priority?: number
  enabled?: boolean
  cache_isolation?: string
  is_default?: boolean
  routes?: { pattern: string; priority?: number }[]
}

export interface UpdateUpstreamRequest {
  display_name?: string
  url?: string
  registry?: string
  username?: string
  password?: string
  skip_tls_verify?: boolean
  priority?: number
  enabled?: boolean
  cache_isolation?: string
  is_default?: boolean
}

export interface TestUpstreamRequest {
  url: string
  registry: string
  username?: string
  password?: string
  skip_tls_verify?: boolean
}

export interface TestUpstreamResponse {
  success: boolean
  message: string
}

export const upstreamsApi = {
  list: () => api.get<Upstream[]>('/upstreams'),
  // Get upstream by name
  get: (name: string) => api.get<Upstream>(`/upstreams/${encodeURIComponent(name)}`),
  create: (data: CreateUpstreamRequest) => api.post<Upstream>('/upstreams', data),
  // Update upstream by name (or id for backward compatibility)
  update: (nameOrId: string | number, data: UpdateUpstreamRequest) =>
    api.put<Upstream>(`/upstreams/${encodeURIComponent(String(nameOrId))}`, data),
  // Delete upstream by name (or id for backward compatibility)
  delete: (nameOrId: string | number) =>
    api.delete(`/upstreams/${encodeURIComponent(String(nameOrId))}`),
  // Health endpoints use name
  getHealth: (name: string) => api.get<UpstreamHealth>(`/upstreams/${encodeURIComponent(name)}/health`),
  getAllHealth: () => api.get<UpstreamHealth[]>('/upstreams/health'),
  getStats: (name: string) => api.get<CacheStats>(`/upstreams/${encodeURIComponent(name)}/stats`),
  // Routes use name
  getRoutes: (name: string) => api.get<UpstreamRoute[]>(`/upstreams/${encodeURIComponent(name)}/routes`),
  addRoute: (name: string, data: { pattern: string; priority?: number }) =>
    api.post<UpstreamRoute>(`/upstreams/${encodeURIComponent(name)}/routes`, data),
  deleteRoute: (upstreamName: string, routeIdx: number) =>
    api.delete(`/upstreams/${encodeURIComponent(upstreamName)}/routes/${routeIdx}`),
  test: (data: TestUpstreamRequest) => api.post<TestUpstreamResponse>('/upstreams/test', data),
  // Reload configuration from file
  reload: () => api.post<{ success: boolean; message: string }>('/upstreams/reload'),
  // Get config file path
  getConfigPath: () => api.get<{ path: string; message: string }>('/upstreams/config-path')
}

export default api
