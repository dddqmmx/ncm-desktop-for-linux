<script setup lang="ts">
import AppIcon from '@renderer/components/common/AppIcon.vue'
import { computed, ref, watch } from 'vue'
import { storeToRefs } from 'pinia'
import { useRouter } from 'vue-router'
import LoginModal from '@renderer/components/overlays/LoginModal.vue'
import CreatePlaylistModal from '@renderer/components/overlays/CreatePlaylistModal.vue'
import { useUserStore } from '@renderer/stores/userStore'
import { useFavoriteStore } from '@renderer/stores/favoriteStore'
import { usePlaylistStore } from '@renderer/stores/playlistStore'
import SongCover from '@renderer/components/media/SongCover.vue'
import UserAvatar from '@renderer/components/media/UserAvatar.vue'

const userStore = useUserStore()
const favoriteStore = useFavoriteStore()
const router = useRouter()
const showLoginModal = ref(false)
const showCreatePlaylistModal = ref(false)
const playlistStore = usePlaylistStore()
const { playlists: createdPlaylists } = storeToRefs(playlistStore)

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
  playlistStore.clearPlaylists()
}

const openCreatePlaylistModal = (): void => {
  if (!userStore.userInfo) {
    showLoginModal.value = true
    return
  }
  showCreatePlaylistModal.value = true
}

// 新建成功后：重新拉一次歌单列表，并直接跳到新歌单
const handlePlaylistCreated = async (playlistId: number): Promise<void> => {
  showCreatePlaylistModal.value = false
  await playlistStore.fetchPlaylists()
  void router.push(`/playlist/${playlistId}`)
}

const uid = computed(() => userStore.userInfo?.account.id)

watch(
  uid,
  (newUid, oldUid) => {
    if (newUid !== oldUid) {
      void playlistStore.fetchPlaylists()
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

  <CreatePlaylistModal
    v-if="showCreatePlaylistModal"
    @close="showCreatePlaylistModal = false"
    @created="handlePlaylistCreated"
  />

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
      <div v-if="userStore.userInfo" class="nav-group-header">
        <span class="nav-group-title">创建的歌单</span>
        <button
          class="nav-group-action"
          type="button"
          title="新建歌单"
          aria-label="新建歌单"
          @click="openCreatePlaylistModal"
        >
          <AppIcon name="plus" :size="16" />
        </button>
      </div>
      <nav v-if="createdPlaylists.length" class="nav-section">
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
      <p v-else-if="userStore.userInfo" class="playlist-empty-hint">还没有歌单，点上面的 + 新建</p>
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
  background: var(--glass-fill);
  backdrop-filter: var(--sys-glass-blur);
  -webkit-backdrop-filter: var(--sys-glass-blur);
  border: var(--glass-border-width) solid var(--glass-border);
  box-shadow:
    var(--glass-shadow),
    inset 0 2px 1px var(--glass-highlight),
    inset 0 -1.5px 1px var(--glass-shade),
    var(--glass-glow);
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
.nav-group-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 8px;
  padding: 0 12px;
  -webkit-app-region: no-drag;
}

.nav-group-title {
  font-size: 12px;
  color: var(--sys-text-tertiary);
  font-weight: 600;
  text-transform: uppercase;
}

.nav-group-action {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  padding: 0;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: var(--sys-text-tertiary);
  cursor: pointer;
  transition:
    color 0.2s ease,
    background-color 0.2s ease;
}

.nav-group-action:hover {
  background: var(--sys-control-hover);
  color: var(--sys-text);
}

.playlist-empty-hint {
  margin: 0 0 24px;
  padding: 0 12px;
  font-size: 12px;
  color: var(--sys-text-disabled);
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  border-radius: 12px;
  /* 默认就用主题色（降饱和后仍可读），不再是一味的中性灰 */
  color: var(--nav-item-fg);
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition:
    color 0.2s ease,
    background-color 0.2s ease;
  text-decoration: none;
}
.nav-item:hover {
  background-color: var(--sys-control-hover);
  color: var(--nav-item-fg-hover);
}
/* 选中态：主题色原色 + 略粗的字重，比默认/悬浮都更亮 */
.nav-item.active {
  background-color: var(--sys-control-selected);
  color: var(--nav-item-fg-active);
  font-weight: 600;
  box-shadow: var(--sys-shadow-soft);
}

.nav-icon {
  width: 20px;
  height: 20px;
  /* 图标跟随 currentColor，色相由导航项统一控制 */
  opacity: 1;
  flex-shrink: 0;
  border-radius: 6px;
  object-fit: cover;
  transition: transform 0.25s cubic-bezier(0.34, 1.56, 0.64, 1);
}
.nav-item.active .nav-icon {
  transform: scale(1.08);
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
  .playlist-empty-hint,
  .username,
  .bubble-menu {
    display: none;
  }
  /* 收窄后标题隐藏，+ 仍要能点到，所以居中摆放 */
  .nav-group-header {
    justify-content: center;
    padding-left: 0;
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
