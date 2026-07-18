<script setup lang="ts">
import AppIcon from '@renderer/components/common/AppIcon.vue'
import { computed, ref, watch } from 'vue'
import LoginModal from '@renderer/components/overlays/LoginModal.vue'
import { useUserStore } from '@renderer/stores/userStore'
import { useFavoriteStore } from '@renderer/stores/favoriteStore'
import { Playlist, PlaylistResponse } from '@renderer/types/userPlaylist'
import SongCover from '@renderer/components/media/SongCover.vue'
import UserAvatar from '@renderer/components/media/UserAvatar.vue'

const userStore = useUserStore()
const favoriteStore = useFavoriteStore()
const showLoginModal = ref(false)
const createdPlaylists = ref<Playlist[]>([])

const toggleLoginModal = (): void => {
  if (!userStore.userInfo) {
    showLoginModal.value = true
  }
}
const handleLoginSuccess = async (): Promise<void> => {
  showLoginModal.value = false
  setTimeout(() => {
    window.location.reload()
  }, 300)
}

const handleLogout = (): void => {
  // 假设 store 中有 logout 方法，没有的话请根据实际情况清除状态
  userStore.logout()
  favoriteStore.clearFavorites()
  createdPlaylists.value = []
}

// 获取歌单的方法
const fetchPlaylists = async (): Promise<void> => {
  const uid = userStore.userInfo?.account.id
  if (!uid) {
    createdPlaylists.value = []
    return
  }

  try {
    const res = (await window.api.user_playlist({
      uid: uid
    })) as { body?: PlaylistResponse }

    if (res.body && res.body.playlist) {
      createdPlaylists.value = res.body.playlist
    }
  } catch (error) {
    console.error('获取歌单失败:', error)
  }
}

const uid = computed(() => userStore.userInfo?.account.id)

watch(
  uid,
  (newUid, oldUid) => {
    if (newUid !== oldUid) {
      fetchPlaylists()
    }
  },
  { immediate: true }
)

const navItems = [
  { id: 'search', name: '搜索', icon: 'search', path: '/search' },
  { id: 1, name: '主页', icon: 'home', path: '/home' },
  { id: 'favorites', name: '喜欢的音乐', icon: 'heart', path: '/favorites' },
  { id: 'local-music', name: '本地音乐', icon: 'music', path: '/local-music' }
  // { id: 'test',   name: '测试界面', icon: 'test', path: '/artist/10000' },
]

const openSettingsWindow = (): void => {
  window.api.open_settings_window()
}
</script>

<template>
  <Transition name="modal-fade">
    <LoginModal
      v-if="showLoginModal"
      @close="showLoginModal = false"
      @login-success="handleLoginSuccess"
    />
  </Transition>

  <aside class="sidebar-floating glass-panel">
    <div class="sidebar-content">
      <!-- 导航区 -->
      <nav class="nav-section first-nav">
        <RouterLink
          v-for="item in navItems"
          :key="item.id"
          :to="item.path"
          class="nav-item"
          active-class="active"
        >
          <AppIcon :name="item.icon === 'test' ? 'flask' : item.icon" class="nav-icon" :size="20" />
          {{ item.name }}
        </RouterLink>
      </nav>

      <!-- 创建的歌单 -->
      <div v-if="createdPlaylists.length" class="nav-group-title">创建的歌单</div>
      <nav class="nav-section">
        <RouterLink
          v-for="item in createdPlaylists"
          :key="item.id"
          :to="`/playlist/${item.id}`"
          class="nav-item"
          active-class="active"
        >
          <div class="nav-icon-wrapper">
            <SongCover :id="item.coverImgUrl" size="40y40" />
          </div>
          <span class="playlist-name">{{ item.name }}</span>
        </RouterLink>
      </nav>
    </div>

    <!-- 用户信息气泡容器 -->
    <div class="user-section">
      <div
        class="profile-bubble"
        :class="{ 'is-logged-in': !!userStore.userInfo }"
        @click="toggleLoginModal"
      >
        <!-- 悬浮出现的菜单 -->
        <div class="bubble-menu">
          <div v-if="userStore.userInfo" class="menu-item">修改个人信息</div>
          <div class="menu-item" @click.stop="openSettingsWindow">设置</div>
          <div v-if="userStore.userInfo" class="menu-item logout" @click.stop="handleLogout">
            退出登录
          </div>
        </div>

        <!-- 底部基础信息 -->
        <div class="bubble-trigger">
          <div class="avatar-wrapper">
            <UserAvatar
              :id="userStore.userInfo?.profile.avatarUrl"
              :rounded="true"
              size="64y64"
              class="avatar"
            />
          </div>
          <span class="username">
            {{ userStore.userInfo?.profile.nickname || '请登录' }}
          </span>
        </div>
      </div>
    </div>
  </aside>
</template>

<style scoped>
.sidebar-floating {
  width: 260px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  margin: 8px;
  border-radius: 16px;
  height: calc(100% - 16px);
  min-height: 0;
  background: var(--sys-surface);
  backdrop-filter: var(--sys-glass-blur);
  border: 1px solid var(--sys-border);
  box-shadow: var(--sys-shadow-soft);
  overflow: hidden;
  -webkit-app-region: drag;
}

.sidebar-content {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 30px 20px 10px 20px;
}

.nav-section {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 24px;
  -webkit-app-region: no-drag;
}
.nav-group-title {
  font-size: 12px;
  color: var(--sys-text-tertiary);
  font-weight: 600;
  margin-bottom: 8px;
  padding-left: 12px;
  text-transform: uppercase;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  border-radius: 12px;
  color: var(--sys-text-secondary);
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
  text-decoration: none;
}
.nav-item:hover {
  background-color: var(--sys-control-hover);
}
.nav-item.active {
  background-color: var(--sys-control-selected);
  color: var(--theme-color-strong);
  box-shadow: var(--sys-shadow-soft);
}

.nav-icon {
  width: 20px;
  height: 20px;
  opacity: 0.7;
  flex-shrink: 0;
  border-radius: 6px;
  object-fit: cover;
}

.nav-icon-wrapper {
  width: 20px;
  height: 20px;
  border-radius: 6px;
  overflow: hidden;
  flex-shrink: 0;
}

.playlist-name {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  flex: 1;
}

/* --- 用户气泡样式 --- */
.user-section {
  padding: 20px;
  margin-top: auto;
}

.profile-bubble {
  position: relative;
  background: var(--sys-control);
  border-radius: 30px; /* 两侧圆润 */
  padding: 6px;
  transition: all 0.4s cubic-bezier(0.165, 0.84, 0.44, 1);
  cursor: pointer;
  /* 核心：内阴影实现“挖洞”感 */
  box-shadow:
    inset 3px 3px 6px rgba(0, 0, 0, 0.1),
    inset -2px -2px 4px rgba(162, 162, 162, 0.7);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  -webkit-app-region: no-drag;
}

/* 触发区域（头像和名字） */
.bubble-trigger {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 6px 12px;
  height: 44px;
  z-index: 2;
}

.avatar-wrapper {
  width: 32px;
  height: 32px;
  flex-shrink: 0;
  border-radius: 50%;
  overflow: hidden;
  box-shadow: 0 2px 5px rgba(0, 0, 0, 0.1);
}

.username {
  font-weight: 600;
  font-size: 13px;
  color: var(--sys-text-secondary);
}

/* 隐藏的菜单 */
.bubble-menu {
  max-height: 0;
  opacity: 0;
  transition: all 0.4s ease;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

/* 登录后悬浮效果 */
.profile-bubble:hover {
  background: var(--sys-control-hover);
  transform: translateY(-5px);
  box-shadow:
    inset 4px 4px 8px rgba(0, 0, 0, 0.12),
    inset -3px -3px 6px rgba(255, 255, 255, 0.8),
    0 10px 20px rgba(0, 0, 0, 0.05);
}

.profile-bubble:hover .bubble-menu {
  max-height: 200px;
  opacity: 1;
  padding-bottom: 8px;
}

/* 菜单项样式 */
.menu-item {
  text-align: center;
  padding: 12px 0;
  font-size: 13px;
  color: var(--sys-text-secondary);
  border-bottom: 1px solid var(--sys-border);
  transition: background 0.2s;
}

.menu-item:hover:first-child {
  border-radius: 30px 30px 0 0;
}

.menu-item:hover {
  background: var(--sys-control);
  color: var(--sys-text);
}

.menu-item.logout {
  color: var(--sys-danger);
  border-bottom: none;
}

/* 移动端适配 */
@media (max-width: 1000px) {
  .sidebar-floating {
    width: 80px;
    align-items: center;
  }
  .sidebar-content {
    padding: 20px 5px;
  }
  .nav-item {
    justify-content: center;
  }
  .nav-group-title,
  .playlist-name,
  .username,
  .bubble-menu {
    display: none;
  }
  .user-section {
    padding: 15px 10px;
  }
  .profile-bubble {
    border-radius: 20px;
  }
  .bubble-trigger {
    padding: 6px;
    justify-content: center;
  }
}

/* 动画 */
.modal-fade-enter-active,
.modal-fade-leave-active {
  transition: opacity 0.3s;
}
.modal-fade-enter-from,
.modal-fade-leave-to {
  opacity: 0;
}
</style>
