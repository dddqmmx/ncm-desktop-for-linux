<script setup lang="ts">
import { ref, reactive } from 'vue'

// 定义设置分类
const tabs = [
  { id: 'general', name: '通用', icon: 'M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4' },
  { id: 'audio', name: '音频输出', icon: 'M15.536 8.464a5 5 0 010 7.072m2.828-9.9a9 9 0 010 12.728M5.586 15H4a1 1 0 01-1-1v-4a1 1 0 011-1h1.586l4.707-4.707C10.923 3.663 12 4.109 12 5v14c0 .891-1.077 1.337-1.707.707L5.586 15z' },
  { id: 'appearance', name: '外观个性化', icon: 'M7 21a4 4 0 01-4-4V5a2 2 0 012-2h4a2 2 0 012 2v12a4 4 0 01-4 4zm0 0h12a2 2 0 002-2v-4a2 2 0 00-2-2h-3' },
  { id: 'library', name: '曲库管理', icon: 'M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z' },
  { id: 'shortcuts', name: '快捷键', icon: 'M9 17.25v1.007a3 3 0 01-.879 2.122L7.5 21h9l-.621-.621A3 3 0 0115 18.257V17.25m6-12V15a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 15V5.25m18 0A2.25 2.25 0 0018.75 3H5.25A2.25 2.25 0 003 5.25m18 0V12a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 12V5.25' },
  { id: 'about', name: '关于软件', icon: 'M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z' }
]

const activeTab = ref('general')

// 详细设置项状态
const settings = reactive({
  autoLaunch: true,
  trayMinimize: true,
  audioEngine: 'native', // native, webapi, auto
  device: 'default',
  quality: 'lossless', // <--- 新增：默认音质
  exclusiveMode: false,
  theme: 'adaptive', // light, dark, adaptive
  acrylic: true,
  accentColor: '#6366f1',
  libPaths: ['C:/Users/Paul/Music', 'D:/Songs/Lossless'],
})


// 版本信息
const appInfo = {
  name: 'ncm-desktop-for-linux',
  version: 'alpha',
  build: 'none',
  author: 'Paul Perkenstein',
  description: '极简风格的高保真本地音乐播放器'
}

const checkingUpdate = ref(false)
const checkUpdate = () => {
  checkingUpdate.value = true
  setTimeout(() => { checkingUpdate.value = false }, 2000)
}
</script>

<template>
  <div class="settings-mask" @click.self="$emit('close')">
    <div class="settings-window glass-morphism-heavy">

      <!-- 侧边栏导航 -->
      <aside class="settings-sidebar">
        <div class="sidebar-header">
          <div class="app-brand">
            <div class="brand-dot"></div>
            <span>SETTINGS</span>
          </div>
        </div>

        <nav class="sidebar-nav">
          <div class="nav-indicator"
            :style="{ transform: `translateY(${tabs.findIndex(t => t.id === activeTab) * 52}px)` }"></div>

          <button v-for="tab in tabs" :key="tab.id" class="nav-item" :class="{ active: activeTab === tab.id }"
            @click="activeTab = tab.id">
            <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" :d="tab.icon" />
            </svg>
            {{ tab.name }}
          </button>
        </nav>
      </aside>

      <!-- 主内容区 -->
      <main class="settings-main">
        <header class="main-header">
          <h2>{{tabs.find(t => t.id === activeTab)?.name}}</h2>
          <button class="close-btn" @click="$emit('close')">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </header>

        <div class="content-scroll">

          <!-- 常规设置 -->
          <div v-if="activeTab === 'general'" class="section-fade">
            <div class="setting-group">
              <label class="group-title">启动与运行</label>
              <div class="control-card">
                <div class="control-row">
                  <div class="info">
                    <span class="primary">开机自启动</span>
                    <span class="secondary">在登录系统时自动启动音乐播放器</span>
                  </div>
                  <div class="switch"><input type="checkbox" v-model="settings.autoLaunch"></div>
                </div>
                <div class="control-row">
                  <div class="info">
                    <span class="primary">最小化至托盘</span>
                    <span class="secondary">关闭主窗口时应用将继续在后台运行</span>
                  </div>
                  <div class="switch"><input type="checkbox" v-model="settings.trayMinimize"></div>
                </div>
              </div>
            </div>
          </div>

          <div v-if="activeTab === 'audio'" class="section-fade">
            <div class="setting-group">
                <label class="group-title">音频质量</label>
                <div class="control-card">
                  <div class="control-row">
                    <div class="info">
                      <span class="primary">默认播放音质</span>
                      <span class="secondary">选择流媒体播放或下载的默认音质级别</span>
                    </div>
                    <select class="modern-select" v-model="settings.quality">
                      <option value="standard">标准 (128kbps)</option>
                      <option value="higher">极高 (320kbps)</option>
                      <option value="lossless">无损 (FLAC/ALAC)</option>
                      <option value="hires">Hi-Res (高解析度音频)</option>
                    </select>
                  </div>
                </div>
              </div>

            <div class="setting-group">
              <label class="group-title">输出架构</label>
              <div class="segmented-slider">
                <div class="slider-bg"
                  :style="{ width: '33.33%', left: settings.audioEngine === 'native' ? '0%' : settings.audioEngine === 'webapi' ? '33.33%' : '66.66%' }">
                </div>
                <button @click="settings.audioEngine = 'native'"
                  :class="{ active: settings.audioEngine === 'native' }">Native</button>
                <button @click="settings.audioEngine = 'webapi'"
                  :class="{ active: settings.audioEngine === 'webapi' }">WebAPI</button>
                <button @click="settings.audioEngine = 'auto'"
                  :class="{ active: settings.audioEngine === 'auto' }">Auto</button>
              </div>
              <p class="tip-text">Native 提供高性能原生输出；WebAPI 具有更佳的系统兼容性。</p>
            </div>

            <div class="setting-group">
              <label class="group-title">设备选择</label>
              <div class="control-card">
                <div class="control-row">
                  <div class="info">
                    <span class="primary">指定输出设备</span>
                    <span class="secondary">选择音频播放的物理端点</span>
                  </div>
                  <select class="modern-select" v-model="settings.device">
                    <option value="default">系统默认输出</option>
                    <option value="speaker">扬声器 (High Definition Audio)</option>
                    <option value="earphone">耳机 (USB Audio Device)</option>
                  </select>
                </div>
                <div class="control-row">
                  <div class="info">
                    <span class="primary">独占输出模式</span>
                    <span class="secondary">允许应用接管音频硬件，减少系统混音干扰</span>
                  </div>
                  <div class="switch"><input type="checkbox" v-model="settings.exclusiveMode"></div>
                </div>
              </div>
            </div>
          </div>

          <!-- 外观个性化 (已修改) -->
          <div v-if="activeTab === 'appearance'" class="section-fade">
            <div class="setting-group">
              <label class="group-title">色彩模式</label>
              <div class="segmented-slider">
                <div class="slider-bg"
                  :style="{ width: '33.33%', left: settings.theme === 'light' ? '0%' : settings.theme === 'dark' ? '33.33%' : '66.66%' }">
                </div>
                <button @click="settings.theme = 'light'" :class="{ active: settings.theme === 'light' }">浅色模式</button>
                <button @click="settings.theme = 'dark'" :class="{ active: settings.theme === 'dark' }">深色模式</button>
                <button @click="settings.theme = 'adaptive'"
                  :class="{ active: settings.theme === 'adaptive' }">跟随系统</button>
              </div>
            </div>

            <div class="setting-group">
              <label class="group-title">视觉效果</label>
              <div class="control-card">
                <div class="control-row">
                  <div class="info">
                    <span class="primary">亚克力 / 云母效果</span>
                    <span class="secondary">启用窗口半透明磨砂背景</span>
                  </div>
                  <div class="switch"><input type="checkbox" v-model="settings.acrylic"></div>
                </div>
                <div class="control-row">
                  <div class="info">
                    <span class="primary">强调色</span>
                  </div>
                  <input type="color" v-model="settings.accentColor" class="color-picker">
                </div>
              </div>
            </div>
          </div>

          <!-- 曲库管理 -->
          <div v-if="activeTab === 'library'" class="section-fade">
            <div class="setting-group">
              <label class="group-title">本地文件夹</label>
              <div class="path-list">
                <div v-for="path in settings.libPaths" :key="path" class="path-item">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                  </svg>
                  <span>{{ path }}</span>
                  <button class="remove-path">移除</button>
                </div>
                <button class="add-path-btn">+ 添加文件夹</button>
              </div>
            </div>
          </div>

          <!-- 关于软件 -->
          <div v-if="activeTab === 'about'" class="section-fade">
            <div class="about-container">
              <div class="about-hero">
                <div class="app-logo-box">
                  <div class="logo-inner"></div>
                </div>
                <h3 class="app-name">{{ appInfo.name }}</h3>
                <p class="app-version">Version {{ appInfo.version }}</p>
              </div>
              <div class="setting-group">
                <div class="control-card">
                  <div class="control-row">
                    <div class="info"><span class="primary">检查更新</span></div>
                    <button class="action-btn" @click="checkUpdate" :disabled="checkingUpdate">
                      {{ checkingUpdate ? '检查中...' : '检查更新' }}
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </div>

        </div>
      </main>
    </div>
  </div>
</template>

<style scoped>
/* 核心玻璃质感 */
.glass-morphism-heavy {
  background: rgba(245, 245, 247, 0.75);
  backdrop-filter: blur(40px) saturate(160%);
  -webkit-backdrop-filter: blur(40px) saturate(160%);
  border: 1px solid rgba(255, 255, 255, 0.5);
  box-shadow: 0 30px 80px rgba(0, 0, 0, 0.15);
}

.settings-mask {
  position: fixed;
  inset: 0;
  z-index: 2000;
  background: rgba(0, 0, 0, 0.15);
  display: flex;
  align-items: center;
  justify-content: center;
}

.settings-window {
  width: 960px;
  height: 640px;
  border-radius: 32px;
  display: flex;
  overflow: hidden;
  animation: modalScaleUp 0.4s cubic-bezier(0.16, 1, 0.3, 1);
}

/* 侧边栏设计 */
.settings-sidebar {
  width: 240px;
  border-right: 1px solid rgba(0, 0, 0, 0.05);
  display: flex;
  flex-direction: column;
  padding: 32px 16px;
}

.app-brand {
  padding: 0 16px 32px;
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 11px;
  font-weight: 800;
  letter-spacing: 1.5px;
  color: rgba(0, 0, 0, 0.4);
}

.brand-dot {
  width: 8px;
  height: 8px;
  background: #111;
  border-radius: 2px;
}

.sidebar-nav {
  position: relative;
  flex: 1;
}

.nav-indicator {
  position: absolute;
  left: 0;
  top: 0;
  width: 100%;
  height: 44px;
  background: rgba(0, 0, 0, 0.04);
  border-radius: 12px;
  transition: transform 0.3s cubic-bezier(0.2, 1, 0.3, 1);
  z-index: 0;
}

.nav-item {
  position: relative;
  width: 100%;
  height: 44px;
  margin-bottom: 8px;
  border: none;
  background: none;
  display: flex;
  align-items: center;
  padding: 0 16px;
  gap: 12px;
  font-size: 14px;
  font-weight: 600;
  color: rgba(0, 0, 0, 0.5);
  cursor: pointer;
  transition: color 0.3s;
  z-index: 1;
}

.nav-item.active {
  color: #111;
}

.nav-icon {
  width: 18px;
  height: 18px;
}

/* 主区域设计 */
.settings-main {
  flex: 1;
  display: flex;
  flex-direction: column;
  background: rgba(255, 255, 255, 0.2);
}

.main-header {
  padding: 32px 48px;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.main-header h2 {
  font-size: 24px;
  font-weight: 800;
  margin: 0;
}

.close-btn {
  background: none;
  border: none;
  color: rgba(0, 0, 0, 0.3);
  cursor: pointer;
  transition: color 0.2s;
}

.content-scroll {
  flex: 1;
  overflow-y: auto;
  padding: 0 48px 48px;
}

.setting-group {
  margin-bottom: 32px;
}

.group-title {
  display: block;
  font-size: 12px;
  font-weight: 700;
  color: rgba(0, 0, 0, 0.3);
  margin-bottom: 12px;
  margin-left: 4px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.control-card {
  background: rgba(255, 255, 255, 0.6);
  border-radius: 20px;
  padding: 8px 20px;
  border: 1px solid rgba(255, 255, 255, 0.5);
}

.control-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 0;
}

.control-row:not(:last-child) {
  border-bottom: 1px solid rgba(0, 0, 0, 0.04);
}

.info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.primary {
  font-size: 15px;
  font-weight: 600;
  color: #111;
}

.secondary {
  font-size: 12px;
  color: rgba(0, 0, 0, 0.4);
}

/* 分段滑块 */
.segmented-slider {
  display: flex;
  position: relative;
  background: rgba(0, 0, 0, 0.05);
  padding: 4px;
  border-radius: 14px;
}

.slider-bg {
  position: absolute;
  top: 4px;
  bottom: 4px;
  background: white;
  border-radius: 10px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08);
  transition: all 0.4s cubic-bezier(0.2, 1, 0.3, 1);
}

.segmented-slider button {
  flex: 1;
  border: none;
  background: none;
  padding: 10px;
  font-size: 13px;
  font-weight: 700;
  color: rgba(0, 0, 0, 0.4);
  cursor: pointer;
  z-index: 1;
  transition: color 0.3s;
}

.segmented-slider button.active {
  color: #111;
}

/* 选择框与颜色 */
.modern-select {
  background: rgba(255, 255, 255, 0.8);
  border: 1px solid rgba(0, 0, 0, 0.1);
  padding: 8px 12px;
  border-radius: 10px;
  font-size: 13px;
  font-weight: 600;
  outline: none;
  min-width: 180px;
}

.color-picker {
  border: none;
  width: 40px;
  height: 24px;
  background: none;
  cursor: pointer;
}

/* 开关 */
input[type="checkbox"] {
  appearance: none;
  width: 44px;
  height: 24px;
  background: rgba(0, 0, 0, 0.1);
  border-radius: 12px;
  position: relative;
  cursor: pointer;
  transition: 0.3s;
}

input[type="checkbox"]:checked {
  background: #000;
}

input[type="checkbox"]::after {
  content: '';
  position: absolute;
  top: 2px;
  left: 2px;
  width: 20px;
  height: 20px;
  background: white;
  border-radius: 50%;
  transition: 0.3s cubic-bezier(0.2, 1, 0.3, 1);
}

input[type="checkbox"]:checked::after {
  transform: translateX(20px);
}

/* 路径管理 */
.path-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.path-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 16px;
  background: rgba(255, 255, 255, 0.4);
  border-radius: 12px;
  font-size: 13px;
}

.remove-path {
  margin-left: auto;
  border: none;
  background: none;
  color: #ef4444;
  font-size: 12px;
  cursor: pointer;
}

.add-path-btn {
  padding: 12px;
  border: 2px dashed rgba(0, 0, 0, 0.1);
  background: none;
  border-radius: 12px;
  color: rgba(0, 0, 0, 0.4);
  font-weight: 600;
  cursor: pointer;
}

/* 关于页面 */
.about-hero {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 20px 0 40px;
}

.app-logo-box {
  width: 80px;
  height: 80px;
  background: linear-gradient(135deg, #6366f1 0%, #a855f7 100%);
  border-radius: 22px;
  padding: 18px;
  margin-bottom: 20px;
}

.logo-inner {
  width: 100%;
  height: 100%;
  background: white;
  mask: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='black' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M9 18V5l12-2v13'%3E%3C/path%3E%3Ccircle cx='6' cy='18' r='3'%3E%3C/circle%3E%3Ccircle cx='18' cy='16' r='3'%3E%3C/circle%3E%3C/svg%3E") no-repeat center;
  -webkit-mask: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='black' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M9 18V5l12-2v13'%3E%3C/path%3E%3Ccircle cx='6' cy='18' r='3'%3E%3C/circle%3E%3Ccircle cx='18' cy='16' r='3'%3E%3C/circle%3E%3C/svg%3E") no-repeat center;
}

.action-btn {
  background: #111;
  color: white;
  border: none;
  padding: 8px 16px;
  border-radius: 10px;
  font-size: 13px;
  cursor: pointer;
}

@keyframes modalScaleUp {
  from {
    opacity: 0;
    transform: scale(0.95) translateY(20px);
  }

  to {
    opacity: 1;
    transform: scale(1) translateY(0);
  }
}

.section-fade {
  animation: fadeIn 0.4s ease-out;
}

@keyframes fadeIn {
  from {
    opacity: 0;
    transform: translateY(10px);
  }

  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.tip-text {
  font-size: 11px;
  color: rgba(0, 0, 0, 0.4);
  margin-top: 10px;
  padding-left: 4px;
}

/* 1. 美化滚动条整体轨道 */
.content-scroll::-webkit-scrollbar {
  width: 6px; /* 纵向滚动条宽度 */
  height: 6px; /* 横向滚动条高度 */
}

/* 2. 滚动条滑块 (Thumb) */
.content-scroll::-webkit-scrollbar-thumb {
  background: rgba(0, 0, 0, 0.1); /* 初始极淡的颜色 */
  border-radius: 10px;
  transition: all 0.3s ease;
}

/* 3. 鼠标悬停在滚动条上时加深 */
.content-scroll::-webkit-scrollbar-thumb:hover {
  background: rgba(0, 0, 0, 0.25);
}

/* 4. 滚动条轨道 (Track) - 保持透明以符合玻璃质感 */
.content-scroll::-webkit-scrollbar-track {
  background: transparent;
}
</style>
