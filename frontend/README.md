# Harbor Cache Web UI

The Harbor Cache frontend is a Vue.js 3 application that provides a web-based management interface for Harbor Cache.

## Features

- **Dashboard**: Real-time cache statistics and performance metrics
- **Cache Management**: View cache status, run cleanup, and clear cache
- **User Management**: Create, edit, and delete user accounts with role-based access
- **Configuration**: Manage runtime configuration settings

## Technology Stack

- **Vue 3** - Progressive JavaScript framework with Composition API
- **TypeScript** - Type-safe JavaScript
- **Vite** - Fast build tool and development server
- **Tailwind CSS** - Utility-first CSS framework
- **Pinia** - State management for Vue
- **Vue Router** - Client-side routing
- **Axios** - HTTP client for API communication
- **Heroicons** - Beautiful hand-crafted SVG icons

## Project Structure

```
frontend/
├── src/
│   ├── api/
│   │   └── client.ts       # API client with type definitions
│   ├── stores/
│   │   └── auth.ts         # Authentication state (Pinia)
│   ├── views/
│   │   ├── LoginView.vue   # Login page
│   │   ├── DashboardView.vue  # Main dashboard
│   │   ├── CacheView.vue   # Cache management
│   │   ├── UsersView.vue   # User management
│   │   └── ConfigView.vue  # Configuration
│   ├── router/
│   │   └── index.ts        # Route definitions
│   ├── App.vue             # Root component with navigation
│   └── main.ts             # Application entry point
├── public/                 # Static assets
├── index.html              # HTML template
├── package.json            # Dependencies and scripts
├── tailwind.config.js      # Tailwind CSS configuration
├── tsconfig.json           # TypeScript configuration
└── vite.config.ts          # Vite configuration
```

## Development

### Prerequisites

- Node.js 18 or later
- npm or yarn
- Harbor Cache backend running (for API calls)

### Setup

```bash
# Install dependencies
npm install

# Start development server
npm run dev
```

The development server runs at `http://localhost:5173` by default.

### API Proxy

During development, API requests are proxied to the backend. Configure the proxy in `vite.config.ts`:

```typescript
export default defineConfig({
  server: {
    proxy: {
      '/api': {
        target: 'http://localhost:5001',
        changeOrigin: true,
      },
    },
  },
});
```

### Building

```bash
# Type check and build for production
npm run build

# Preview production build
npm run preview
```

Built files are output to `dist/` and should be copied to `static/` in the project root for the backend to serve.

### Type Checking

```bash
# Run type checking
npx vue-tsc --noEmit
```

## Components

### Views

#### LoginView
- JWT authentication flow
- Credential form with validation
- Error handling for failed login attempts
- Redirect to dashboard on success

#### DashboardView
- Cache statistics overview (total size, entry count)
- Breakdown by type (manifests, blobs)
- Cache performance metrics (hit rate, hits, misses)
- Refresh functionality

#### CacheView
- Detailed cache statistics
- Cleanup expired entries (admin only)
- Clear all cache with confirmation (admin only)
- Real-time status updates

#### UsersView
- User list with role indicators
- Create new users with role selection
- Edit existing users (role, password)
- Delete users with confirmation

#### ConfigView
- View all configuration entries
- Edit configuration values
- Add new configuration entries
- Delete configuration entries

### Shared Components

#### Navigation (in App.vue)
- Sidebar navigation
- Role-based menu visibility
- User info and logout

## API Client

The API client (`src/api/client.ts`) provides typed methods for all backend endpoints:

```typescript
// Authentication
authApi.login(username, password)

// Cache operations
cacheApi.getStats()
cacheApi.cleanup()
cacheApi.clear()

// User management
usersApi.list()
usersApi.get(id)
usersApi.create(data)
usersApi.update(id, data)
usersApi.delete(id)

// Configuration
configApi.list()
configApi.get(key)
configApi.update(data)
configApi.delete(key)
```

## State Management

Authentication state is managed with Pinia (`src/stores/auth.ts`):

```typescript
const authStore = useAuthStore()

// Login
await authStore.login(username, password)

// Check authentication
if (authStore.isAuthenticated) {
  // ...
}

// Check admin role
if (authStore.isAdmin) {
  // ...
}

// Logout
authStore.logout()
```

## Styling

Tailwind CSS is used for styling with a custom theme:

```javascript
// tailwind.config.js
module.exports = {
  theme: {
    extend: {
      colors: {
        primary: {
          // Custom blue color palette
        },
      },
    },
  },
};
```

Common utility classes are defined in `src/style.css`:

```css
.btn { /* Base button styles */ }
.btn-primary { /* Primary button */ }
.btn-secondary { /* Secondary button */ }
.btn-danger { /* Danger/delete button */ }
.card { /* Card container */ }
.input { /* Form input */ }
.label { /* Form label */ }
```

## Routing

Routes are protected by authentication guards:

```typescript
// src/router/index.ts
router.beforeEach((to, from, next) => {
  const authStore = useAuthStore()

  if (to.meta.requiresAuth && !authStore.isAuthenticated) {
    next('/login')
  } else if (to.meta.requiresAdmin && !authStore.isAdmin) {
    next('/')
  } else {
    next()
  }
})
```

## Error Handling

API errors follow the OCI Distribution Spec format:

```json
{
  "errors": [
    {
      "code": "UNAUTHORIZED",
      "message": "Invalid credentials"
    }
  ]
}
```

Components extract and display error messages:

```typescript
catch (err: any) {
  error.value = err.response?.data?.errors?.[0]?.message || 'Operation failed'
}
```

## Production Deployment

1. Build the frontend:
   ```bash
   npm run build
   ```

2. Copy to backend static directory:
   ```bash
   cp -r dist/* ../static/
   ```

3. The backend serves static files automatically from the `/static` directory.

## Browser Support

- Chrome/Edge 90+
- Firefox 88+
- Safari 14+

## Contributing

See the main project [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines.

### Frontend-Specific Guidelines

- Use Composition API with `<script setup>`
- Define TypeScript types for all props and API responses
- Follow Vue.js style guide recommendations
- Keep components focused and reusable
- Use Tailwind utility classes; avoid custom CSS when possible
