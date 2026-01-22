# Harbor Cache Web UI Guide

This guide covers the web-based management interface for Harbor Cache.

## Table of Contents

- [Accessing the Web UI](#accessing-the-web-ui)
- [Login](#login)
- [Dashboard](#dashboard)
- [Cache Management](#cache-management)
- [User Management](#user-management)
- [Configuration](#configuration)
- [Navigation](#navigation)

## Accessing the Web UI

The Harbor Cache web interface is accessible at the same address as the registry:

```
http://harbor-cache.example.com:5001
```

Or with TLS enabled:

```
https://harbor-cache.example.com:5001
```

### Supported Browsers

The web UI works best with modern browsers:
- Chrome/Edge 90+
- Firefox 88+
- Safari 14+

## Login

### Authentication

When you first access the web UI, you'll be presented with a login screen.

**Default Credentials:**
- **Username:** `admin`
- **Password:** `admin`

**Important:** Change the default password immediately after first login in production environments.

### Login Process

1. Enter your username
2. Enter your password
3. Click "Sign in"

After successful authentication, you'll receive a JWT token that's stored in your browser. This token:
- Expires after 24 hours
- Is automatically included in all API requests
- Must be renewed by logging in again after expiration

### Session Expiration

If your session expires, you'll be automatically redirected to the login page. Simply log in again to continue.

## Dashboard

The Dashboard provides an at-a-glance overview of your Harbor Cache instance.

### Statistics Cards

The top section displays four key metrics:

| Metric | Description |
|--------|-------------|
| **Total Size** | Current size of all cached data (e.g., "1.00 GB") |
| **Total Entries** | Number of cached items (manifests + blobs) |
| **Manifests** | Number of cached image manifests |
| **Blobs** | Number of cached image layers and config blobs |

### Cache Performance Section

Below the statistics cards, you'll find cache performance metrics:

| Metric | Description |
|--------|-------------|
| **Hit Rate** | Percentage of requests served from cache (aim for >70%) |
| **Cache Hits** | Total number of requests served from cache |
| **Cache Misses** | Total number of requests fetched from upstream |

### Interpreting the Data

**Good performance indicators:**
- Hit rate above 70%
- Steady increase in cache entries
- Growing cache size (until max size reached)

**Potential issues:**
- Very low hit rate might indicate:
  - Users pulling many unique images
  - Cache being cleared frequently
  - Short retention period
- High miss count with low entries might indicate storage issues

### Refresh Button

Click the "Refresh" button to fetch the latest statistics. The page does not auto-refresh to avoid unnecessary API calls.

## Cache Management

Navigate to Cache Management via the sidebar to manage cached artifacts.

### Viewing Cache Statistics

The Cache Management page shows the same statistics as the Dashboard, plus action buttons for cache operations.

### Cache Actions

**Available to administrators only:**

#### Cleanup Expired Entries

This operation removes entries that exceed the configured retention period.

1. Click "Run Cleanup"
2. Wait for the operation to complete
3. A success message shows how many entries were cleaned

**When to use:**
- When you want to free up space without clearing everything
- To enforce retention policies immediately
- Before maintenance windows

#### Clear All Cache

This operation removes ALL cached entries. Use with caution.

1. Click "Clear Cache"
2. A confirmation dialog appears
3. Click "Clear All" to confirm

**Warning:** This action cannot be undone. All cached images will need to be re-fetched from the upstream registry.

**When to use:**
- When migrating to a new upstream registry
- To reset cache statistics
- When troubleshooting caching issues

### Access Control

Cache actions require admin privileges. Users with `read-only` or `read-write` roles can view statistics but cannot perform cleanup or clear operations.

## User Management

Navigate to User Management via the sidebar to manage user accounts.

### User List

The user list displays:

| Column | Description |
|--------|-------------|
| **Username** | The user's login name |
| **Role** | Permission level (admin, read-write, read-only) |
| **Created** | When the account was created |
| **Actions** | Edit and delete buttons |

### User Roles

| Role | Permissions |
|------|-------------|
| **admin** | Full access: manage users, configuration, cache, pull and push images |
| **read-write** | Pull and push images through the cache |
| **read-only** | Pull images only (cannot push) |

### Creating a User

1. Click "Add User"
2. Fill in the form:
   - **Username:** Unique identifier (cannot be changed later)
   - **Password:** Initial password for the user
   - **Role:** Select from dropdown
3. Click "Create"

**Password requirements:**
- Minimum 4 characters
- Any characters allowed

### Editing a User

1. Click the pencil icon next to the user
2. Modify the fields:
   - **Role:** Can be changed
   - **Password:** Leave empty to keep current password
3. Click "Save"

**Note:** Usernames cannot be changed after creation.

### Deleting a User

1. Click the trash icon next to the user
2. Confirm the deletion in the dialog
3. User is immediately removed

**Warning:** Deletion is permanent and cannot be undone.

**Note:** You cannot delete your own account while logged in.

## Configuration

Navigate to Configuration via the sidebar to manage runtime settings.

### Configuration Entries

The configuration page displays a table of key-value pairs:

| Column | Description |
|--------|-------------|
| **Key** | Configuration key (e.g., `cache.retention_days`) |
| **Value** | Current value |
| **Updated** | When the entry was last modified |

### Available Configuration Keys

| Key | Description | Default |
|-----|-------------|---------|
| `cache.max_size` | Maximum cache size in bytes | 10737418240 (10 GB) |
| `cache.retention_days` | Days to retain cached entries | 30 |
| `cache.eviction_policy` | Eviction strategy (lru, lfu, fifo) | lru |

### Editing Configuration

1. Click "Edit Config"
2. The edit modal shows all current entries
3. Modify values as needed
4. Click "Add Entry" to add new configuration
5. Click "Save Changes"

**Adding a new entry:**
1. Click "Add Entry" in the edit modal
2. Enter a key (e.g., `cache.retention_days`)
3. Enter a value (e.g., `60`)
4. Click "Save Changes"

### Deleting Configuration Entries

1. Click the trash icon next to an entry in the table
2. Confirm the deletion

### Configuration vs. File Settings

Runtime configuration stored in the database:
- Takes effect immediately
- Persists across restarts
- Can be modified via API or UI

Configuration file (`config.toml`):
- Read at startup
- Used for settings not in database
- Includes sensitive settings (JWT secret, upstream credentials)

**Precedence:** Database settings override file settings for supported keys.

## Navigation

### Sidebar Menu

The sidebar provides navigation to all sections:

| Menu Item | Description | Required Role |
|-----------|-------------|---------------|
| **Dashboard** | Overview and statistics | Any |
| **Cache** | Cache management | Any (actions: admin) |
| **Users** | User management | admin |
| **Config** | Configuration | admin |

### User Menu

The top-right corner shows:
- Current username
- Logout option

Click your username to access account options or log out.

### Responsive Design

The web UI is responsive:
- **Desktop (>1024px):** Full sidebar visible
- **Tablet (768-1024px):** Collapsible sidebar
- **Mobile (<768px):** Hamburger menu for navigation

## Keyboard Shortcuts

The web UI supports standard keyboard navigation:

| Key | Action |
|-----|--------|
| `Tab` | Navigate between elements |
| `Enter` | Activate buttons/submit forms |
| `Escape` | Close modals |

## Troubleshooting

### Cannot Log In

1. Verify credentials are correct
2. Check if authentication is enabled in configuration
3. Clear browser cache and cookies
4. Try a different browser

### Statistics Not Loading

1. Check network connectivity
2. Verify the API is responding (`/health` endpoint)
3. Check browser console for errors
4. Ensure your token hasn't expired

### Actions Not Available

1. Verify you have admin role
2. Check if you're logged in
3. Refresh the page to update permissions

### Slow Performance

1. Check network latency to server
2. Monitor server resources (CPU, memory)
3. Consider reducing cache size if database is large

## Next Steps

- [User Guide](user-guide.md) - Learn to use Harbor Cache with Docker
- [API Reference](api-reference.md) - Automate operations via API
- [Configuration](configuration.md) - Detailed configuration options
