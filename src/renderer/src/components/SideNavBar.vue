<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import LoginModal from './LoginModal.vue';
import { useUserStore } from '@renderer/stores/userStore';
import { Playlist, PlaylistResponse } from '@renderer/types/userPlaylist';
import Settings from './Settings.vue';

const userStore = useUserStore()
const showLoginModal = ref(false)
const showSettingsModal = ref(false)
const createdPlaylists = ref<Playlist[]>([])

const toggleLoginModal = () => {
  if (!userStore.userInfo) {
    showLoginModal.value = true
  }
}
const handleLoginSuccess = async () => {
  showLoginModal.value = false
  setTimeout(() => {
    window.location.reload()
  }, 300)
}

const handleLogout = () => {
  // 假设 store 中有 logout 方法，没有的话请根据实际情况清除状态
  userStore.logout()
  createdPlaylists.value = []
}

// 获取歌单的方法
const fetchPlaylists = async () => {
  const uid = userStore.userInfo?.account.id
  if (!uid) {
    createdPlaylists.value = []
    return
  }

  try {
    const res = await window.api.user_playlist({
      uid: uid,
    }) as { body?: PlaylistResponse }

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
]


</script>

<template>
  <Transition name="modal-fade">
    <LoginModal
      v-if="showLoginModal"
      @close="showLoginModal = false"
      @login-success="handleLoginSuccess"/>
  </Transition>

  <Transition name="modal-fade">
    <Settings
      v-if="showSettingsModal"
      @close="showSettingsModal = false"
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
          <svg v-if="item.icon === 'search'" class="nav-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" /></svg>
          <svg v-if="item.icon === 'home'" class="nav-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6" /></svg>
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
          <img class="nav-icon" :src="item.coverImgUrl" alt="playlist" />
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
        <!-- 悬浮出现的菜单 (仅登录后) -->
        <div v-if="userStore.userInfo" class="bubble-menu">
          <div class="menu-item">修改个人信息</div>
          <div class="menu-item" @click.stop="showSettingsModal = true">设置</div>
          <div class="menu-item logout" @click.stop="handleLogout">退出登录</div>
        </div>

        <!-- 底部基础信息 -->
        <div class="bubble-trigger">
          <img
            :src="userStore.userInfo?.profile.avatarUrl || 'https://placehold.co/40x40/ddd/888?text=U'"
            alt="user"
            class="avatar"
          >
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
  margin-right: 16px;
  border-radius: 0 24px 24px 0;
  height: 100%;
  background: rgba(245, 245, 245, 0.65);
  backdrop-filter: blur(40px) saturate(180%);
  border: 1px solid rgba(255, 255, 255, 0.8);
  box-shadow: 0 10px 40px rgba(0, 0, 0, 0.04);
  overflow: hidden;
}

.sidebar-content {
  flex: 1;
  overflow-y: auto;
  padding: 30px 20px 10px 20px;
  scrollbar-width: none;
}
.sidebar-content::-webkit-scrollbar { display: none; }

.nav-section { display: flex; flex-direction: column; gap: 4px; margin-bottom: 24px; }
.nav-group-title { font-size: 12px; color: rgba(0,0,0,0.4); font-weight: 600; margin-bottom: 8px; padding-left: 12px; text-transform: uppercase; }

.nav-item {
  display: flex; align-items: center; gap: 12px; padding: 10px 12px; border-radius: 12px;
  color: #444; font-size: 14px; font-weight: 500; cursor: pointer; transition: all 0.2s;
  text-decoration: none;
}
.nav-item:hover { background-color: rgba(255, 255, 255, 0.6); }
.nav-item.active { background-color: #fff; color: #000; box-shadow: 0 4px 12px rgba(0,0,0,0.05); }

.nav-icon { width: 20px; height: 20px; opacity: 0.7; flex-shrink: 0; border-radius: 6px; object-fit: cover; }
.playlist-name { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; flex: 1; }

/* --- 用户气泡样式 --- */
.user-section {
  padding: 20px;
  margin-top: auto;
}

.profile-bubble {
  position: relative;
  background: #e8e8e8; /* 气泡背景色 */
  border-radius: 30px; /* 两侧圆润 */
  padding: 6px;
  transition: all 0.4s cubic-bezier(0.165, 0.84, 0.44, 1);
  cursor: pointer;
  /* 核心：内阴影实现“挖洞”感 */
  box-shadow:
    inset 3px 3px 6px rgba(0, 0, 0, 0.1),
    inset -2px -2px 4px rgba(255, 255, 255, 0.7);
  display: flex;
  flex-direction: column;
  overflow: hidden;
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

.avatar {
  width: 32px;
  height: 32px;
  border-radius: 50%;
  background: #ccc;
  box-shadow: 0 2px 5px rgba(0,0,0,0.1);
}

.username {
  font-weight: 600;
  font-size: 13px;
  color: #555;
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
.profile-bubble.is-logged-in:hover {
  background: #e0e0e0;
  transform: translateY(-5px);
  box-shadow:
    inset 4px 4px 8px rgba(0, 0, 0, 0.12),
    inset -3px -3px 6px rgba(255, 255, 255, 0.8),
    0 10px 20px rgba(0,0,0,0.05);
}

.profile-bubble.is-logged-in:hover .bubble-menu {
  max-height: 200px;
  opacity: 1;
  padding-bottom: 8px;
}

/* 菜单项样式 */
.menu-item {
  text-align: center;
  padding: 12px 0;
  font-size: 13px;
  color: #666;
  border-bottom: 1px solid rgba(0, 0, 0, 0.03); /* 自然分割线 */
  transition: background 0.2s;
}

.menu-item:hover {
  background: rgba(0, 0, 0, 0.02);
  color: #000;
}

.menu-item.logout {
  color: #d93025;
  border-bottom: none;
}

/* 移动端适配 */
@media (max-width: 1000px) {
  .sidebar-floating { width: 80px; align-items: center; }
  .sidebar-content { padding: 20px 5px; }
  .nav-item { justify-content: center; }
  .nav-group-title, .playlist-name, .username, .bubble-menu { display: none; }
  .user-section { padding: 15px 10px; }
  .profile-bubble { border-radius: 20px; }
  .bubble-trigger { padding: 6px; justify-content: center; }
}

/* 动画 */
.modal-fade-enter-active, .modal-fade-leave-active { transition: opacity 0.3s; }
.modal-fade-enter-from, .modal-fade-leave-to { opacity: 0; }
</style>
