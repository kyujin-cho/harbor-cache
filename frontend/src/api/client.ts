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
export interface Upstream {
  id: number
  name: string
  display_name: string
  url: string
  registry: string
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
  get: (id: number) => api.get<Upstream>(`/upstreams/${id}`),
  create: (data: CreateUpstreamRequest) => api.post<Upstream>('/upstreams', data),
  update: (id: number, data: UpdateUpstreamRequest) => api.put<Upstream>(`/upstreams/${id}`, data),
  delete: (id: number) => api.delete(`/upstreams/${id}`),
  getHealth: (id: number) => api.get<UpstreamHealth>(`/upstreams/${id}/health`),
  getAllHealth: () => api.get<UpstreamHealth[]>('/upstreams/health'),
  getStats: (id: number) => api.get<CacheStats>(`/upstreams/${id}/stats`),
  getRoutes: (id: number) => api.get<UpstreamRoute[]>(`/upstreams/${id}/routes`),
  addRoute: (id: number, data: { pattern: string; priority?: number }) =>
    api.post<UpstreamRoute>(`/upstreams/${id}/routes`, data),
  deleteRoute: (upstreamId: number, routeId: number) =>
    api.delete(`/upstreams/${upstreamId}/routes/${routeId}`),
  test: (data: TestUpstreamRequest) => api.post<TestUpstreamResponse>('/upstreams/test', data)
}

export default api
