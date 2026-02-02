<script setup lang="ts">
import { ref, reactive } from 'vue'
import SettingsSidebar from './SettingsSidebar.vue'
import SettingGroup from './SettingGroup.vue'
import SettingRow from './SettingRow.vue'
import SegmentedSlider from './SegmentedSlider.vue'

const emit = defineEmits(['close'])

const tabs = [
  { id: 'general', name: '通用', icon: 'M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4' },
  { id: 'audio', name: '音频输出', icon: 'M15.536 8.464a5 5 0 010 7.072m2.828-9.9a9 9 0 010 12.728M5.586 15H4a1 1 0 01-1-1v-4a1 1 0 011-1h1.586l4.707-4.707C10.923 3.663 12 4.109 12 5v14c0 .891-1.077 1.337-1.707.707L5.586 15z' },
  { id: 'appearance', name: '外观个性化', icon: 'M7 21a4 4 0 01-4-4V5a2 2 0 012-2h4a2 2 0 012 2v12a4 4 0 01-4 4zm0 0h12a2 2 0 002-2v-4a2 2 0 00-2-2h-3' },
  { id: 'library', name: '曲库管理', icon: 'M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z' },
  { id: 'shortcuts', name: '快捷键', icon: 'M9 17.25v1.007a3 3 0 01-.879 2.122L7.5 21h9l-.621-.621A3 3 0 0115 18.257V17.25m6-12V15a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 15V5.25m18 0A2.25 2.25 0 0018.75 3H5.25A2.25 2.25 0 003 5.25m18 0V12a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 12V5.25' },
  { id: 'about', name: '关于软件', icon: 'M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z' }
]

const activeTab = ref('general')
const settings = reactive({
  autoLaunch: true,
  trayMinimize: true,
  audioEngine: 'native',
  device: 'default',
  quality: 'lossless',
  exclusiveMode: false,
  theme: 'adaptive',
  acrylic: true,
  accentColor: '#6366f1',
  libPaths: ['C:/Users/Paul/Music', 'D:/Songs/Lossless'],
})

const appInfo = {
  name: 'ncm-desktop-for-linux',
  version: 'alpha',
  author: 'Paul Perkenstein'
}

const checkingUpdate = ref(false)
const checkUpdate = () => {
  checkingUpdate.value = true
  setTimeout(() => { checkingUpdate.value = false }, 2000)
}
</script>

<template>
  <div class="settings-mask" @click.self="emit('close')">
    <div class="settings-window glass-morphism-heavy">

      <SettingsSidebar :tabs="tabs" v-model:activeTab="activeTab" />

      <main class="settings-main">
        <header class="main-header">
          <h2>{{ tabs.find(t => t.id === activeTab)?.name }}</h2>
          <button class="close-btn" @click="emit('close')">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </header>

        <div class="content-scroll">
          <!-- 通用 -->
          <div v-if="activeTab === 'general'" class="section-fade">
            <SettingGroup title="启动与运行">
              <SettingRow title="开机自启动" description="在登录系统时自动启动音乐播放器">
                <input type="checkbox" v-model="settings.autoLaunch" class="modern-switch">
              </SettingRow>
              <SettingRow title="最小化至托盘" description="关闭主窗口时应用将继续在后台运行">
                <input type="checkbox" v-model="settings.trayMinimize" class="modern-switch">
              </SettingRow>
            </SettingGroup>
          </div>

          <!-- 音频 -->
          <div v-if="activeTab === 'audio'" class="section-fade">
            <SettingGroup title="音频质量">
              <SettingRow title="默认播放音质" description="选择流媒体播放或下载的默认音质级别">
                <select class="modern-select" v-model="settings.quality">
                  <option value="standard">标准 (128kbps)</option>
                  <option value="higher">极高 (320kbps)</option>
                  <option value="lossless">无损 (FLAC/ALAC)</option>
                  <option value="hires">Hi-Res (高解析度音频)</option>
                </select>
              </SettingRow>
            </SettingGroup>

            <SettingGroup title="输出架构" tip="Native 提供高性能原生输出；WebAPI 具有更佳的系统兼容性。" noCard>
              <SegmentedSlider
                v-model="settings.audioEngine"
                :options="[{label:'Native', value:'native'}, {label:'WebAPI', value:'webapi'}, {label:'Auto', value:'auto'}]"
              />
            </SettingGroup>

            <SettingGroup title="设备选择">
              <SettingRow title="指定输出设备" description="选择音频播放的物理端点">
                <select class="modern-select" v-model="settings.device">
                  <option value="default">系统默认输出</option>
                  <option value="speaker">扬声器 (High Definition Audio)</option>
                  <option value="earphone">耳机 (USB Audio Device)</option>
                </select>
              </SettingRow>
              <SettingRow title="独占输出模式" description="允许应用接管音频硬件，减少系统混音干扰">
                <input type="checkbox" v-model="settings.exclusiveMode" class="modern-switch">
              </SettingRow>
            </SettingGroup>
          </div>

          <!-- 外观 -->
          <div v-if="activeTab === 'appearance'" class="section-fade">
            <SettingGroup title="色彩模式" noCard>
              <SegmentedSlider
                v-model="settings.theme"
                :options="[{label:'浅色模式', value:'light'}, {label:'深色模式', value:'dark'}, {label:'跟随系统', value:'adaptive'}]"
              />
            </SettingGroup>
            <SettingGroup title="视觉效果">
              <SettingRow title="亚克力 / 云母效果" description="启用窗口半透明磨砂背景">
                <input type="checkbox" v-model="settings.acrylic" class="modern-switch">
              </SettingRow>
              <SettingRow title="强调色">
                <input type="color" v-model="settings.accentColor" class="color-picker">
              </SettingRow>
            </SettingGroup>
          </div>

          <!-- 曲库 -->
          <div v-if="activeTab === 'library'" class="section-fade">
            <SettingGroup title="本地文件夹" noCard>
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
            </SettingGroup>
          </div>

          <!-- 关于 -->
          <div v-if="activeTab === 'about'" class="section-fade">
            <div class="about-hero">
              <div class="app-logo-box"><div class="logo-inner"></div></div>
              <h3 class="app-name">{{ appInfo.name }}</h3>
              <p class="app-version">Version {{ appInfo.version }}</p>
            </div>
            <SettingGroup title="">
              <SettingRow title="检查更新">
                <button class="action-btn" @click="checkUpdate" :disabled="checkingUpdate">
                  {{ checkingUpdate ? '检查中...' : '检查更新' }}
                </button>
              </SettingRow>
            </SettingGroup>
          </div>
        </div>
      </main>
    </div>
  </div>
</template>

<style scoped>
/* 保持原有核心样式 */
.settings-mask {
  position: fixed; inset: 0; z-index: 2000;
  background: rgba(0, 0, 0, 0.15);
  display: flex; align-items: center; justify-content: center;
}
.settings-window {
  width: 960px; height: 640px; border-radius: 32px;
  display: flex; overflow: hidden;
  animation: modalScaleUp 0.4s cubic-bezier(0.16, 1, 0.3, 1);
}
.glass-morphism-heavy {
  background: rgba(245, 245, 247, 0.75);
  backdrop-filter: blur(40px) saturate(160%);
  -webkit-backdrop-filter: blur(40px) saturate(160%);
  border: 1px solid rgba(255, 255, 255, 0.5);
  box-shadow: 0 30px 80px rgba(0, 0, 0, 0.15);
}
.settings-main { flex: 1; display: flex; flex-direction: column; background: rgba(255, 255, 255, 0.2); }
.main-header { padding: 32px 48px; display: flex; justify-content: space-between; align-items: center; }
.main-header h2 { font-size: 24px; font-weight: 800; margin: 0; }
.close-btn { background: none; border: none; color: rgba(0, 0, 0, 0.3); cursor: pointer; }
.content-scroll { flex: 1; overflow-y: auto; padding: 0 48px 48px; }

/* 表单组件样式 */
.modern-switch {
  appearance: none; width: 44px; height: 24px; background: rgba(0, 0, 0, 0.1);
  border-radius: 12px; position: relative; cursor: pointer; transition: 0.3s;
}
.modern-switch:checked { background: #000; }
.modern-switch::after {
  content: ''; position: absolute; top: 2px; left: 2px; width: 20px; height: 20px;
  background: white; border-radius: 50%; transition: 0.3s cubic-bezier(0.2, 1, 0.3, 1);
}
.modern-switch:checked::after { transform: translateX(20px); }

.modern-select {
  background: rgba(255, 255, 255, 0.8); border: 1px solid rgba(0, 0, 0, 0.1);
  padding: 8px 12px; border-radius: 10px; font-size: 13px; font-weight: 600; outline: none; min-width: 180px;
}
.color-picker { border: none; width: 40px; height: 24px; background: none; cursor: pointer; }

/* 业务特定样式 */
.path-list { display: flex; flex-direction: column; gap: 8px; }
.path-item {
  display: flex; align-items: center; gap: 12px; padding: 12px 16px;
  background: rgba(255, 255, 255, 0.4); border-radius: 12px; font-size: 13px;
}
.remove-path { margin-left: auto; border: none; background: none; color: #ef4444; cursor: pointer; }
.add-path-btn { padding: 12px; border: 2px dashed rgba(0, 0, 0, 0.1); background: none; border-radius: 12px; color: rgba(0, 0, 0, 0.4); font-weight: 600; cursor: pointer; }

.about-hero { display: flex; flex-direction: column; align-items: center; padding: 20px 0 40px; }
.app-logo-box { width: 80px; height: 80px; background: linear-gradient(135deg, #6366f1 0%, #a855f7 100%); border-radius: 22px; padding: 18px; margin-bottom: 20px; }
.logo-inner { width: 100%; height: 100%; background: white; -webkit-mask: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='black' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M9 18V5l12-2v13'%3E%3C/path%3E%3Ccircle cx='6' cy='18' r='3'%3E%3C/circle%3E%3Ccircle cx='18' cy='16' r='3'%3E%3C/circle%3E%3C/svg%3E") no-repeat center; }
.action-btn { background: #111; color: white; border: none; padding: 8px 16px; border-radius: 10px; font-size: 13px; cursor: pointer; }

/* 动画 */
@keyframes modalScaleUp { from { opacity: 0; transform: scale(0.95) translateY(20px); } to { opacity: 1; transform: scale(1) translateY(0); } }
.section-fade { animation: fadeIn 0.4s ease-out; }
@keyframes fadeIn { from { opacity: 0; transform: translateY(10px); } to { opacity: 1; transform: translateY(0); } }

/* 滚动条美化 */
.content-scroll::-webkit-scrollbar { width: 6px; }
.content-scroll::-webkit-scrollbar-thumb { background: rgba(0, 0, 0, 0.1); border-radius: 10px; }
.content-scroll::-webkit-scrollbar-thumb:hover { background: rgba(0, 0, 0, 0.25); }
</style>
