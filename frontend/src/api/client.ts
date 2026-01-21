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

export const cacheApi = {
  getStats: () => api.get<CacheStats>('/cache/stats'),
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

export const configApi = {
  list: () => api.get<ConfigEntry[]>('/config'),
  get: (key: string) => api.get<ConfigEntry>(`/config/${key}`),
  update: (data: UpdateConfigRequest) => api.put<{ updated: number }>('/config', data),
  delete: (key: string) => api.delete(`/config/${key}`)
}

export default api
