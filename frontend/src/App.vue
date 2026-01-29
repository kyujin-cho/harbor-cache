<script setup lang="ts">
import { RouterView, RouterLink, useRoute } from 'vue-router'
import { useAuthStore } from './stores/auth'
import {
  HomeIcon,
  ArchiveBoxIcon,
  UsersIcon,
  Cog6ToothIcon,
  ArrowRightOnRectangleIcon,
  ClipboardDocumentListIcon,
  ServerStackIcon
} from '@heroicons/vue/24/outline'

const authStore = useAuthStore()
const route = useRoute()

function handleLogout() {
  authStore.logout()
}

const navigation = [
  { name: 'Dashboard', to: '/', icon: HomeIcon },
  { name: 'Cache', to: '/cache', icon: ArchiveBoxIcon },
  { name: 'Upstreams', to: '/upstreams', icon: ServerStackIcon, adminOnly: true },
  { name: 'Users', to: '/users', icon: UsersIcon, adminOnly: true },
  { name: 'Config', to: '/config', icon: Cog6ToothIcon, adminOnly: true },
  { name: 'Logs', to: '/logs', icon: ClipboardDocumentListIcon, adminOnly: true }
]
</script>

<template>
  <div class="min-h-screen bg-gray-50">
    <!-- Login page (no sidebar) -->
    <template v-if="route.name === 'login'">
      <RouterView />
    </template>

    <!-- Main layout with sidebar -->
    <template v-else>
      <div class="flex min-h-screen">
        <!-- Sidebar -->
        <div class="w-64 bg-gray-900 text-white flex flex-col">
          <!-- Logo -->
          <div class="p-4 border-b border-gray-800">
            <h1 class="text-xl font-bold flex items-center gap-2">
              <ArchiveBoxIcon class="h-6 w-6 text-primary-400" />
              Harbor Cache
            </h1>
          </div>

          <!-- Navigation -->
          <nav class="flex-1 p-4 space-y-1">
            <template v-for="item in navigation" :key="item.name">
              <RouterLink
                v-if="!item.adminOnly || authStore.isAdmin"
                :to="item.to"
                class="flex items-center gap-3 px-3 py-2 rounded-md text-sm font-medium transition-colors"
                :class="route.path === item.to
                  ? 'bg-gray-800 text-white'
                  : 'text-gray-300 hover:bg-gray-800 hover:text-white'"
              >
                <component :is="item.icon" class="h-5 w-5" />
                {{ item.name }}
              </RouterLink>
            </template>
          </nav>

          <!-- User info & logout -->
          <div class="p-4 border-t border-gray-800">
            <div class="text-sm text-gray-400 mb-2">
              Logged in as <span class="text-white font-medium">{{ authStore.user?.username }}</span>
              <span class="ml-1 text-xs px-1.5 py-0.5 rounded bg-gray-700">{{ authStore.user?.role }}</span>
            </div>
            <button
              @click="handleLogout"
              class="flex items-center gap-2 text-sm text-gray-400 hover:text-white transition-colors"
            >
              <ArrowRightOnRectangleIcon class="h-4 w-4" />
              Sign out
            </button>
          </div>
        </div>

        <!-- Main content -->
        <div class="flex-1 overflow-auto">
          <RouterView />
        </div>
      </div>
    </template>
  </div>
</template>
