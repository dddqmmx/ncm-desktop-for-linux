<script setup lang="ts">
import { ref, reactive, onUnmounted, watch } from 'vue'
import { useUserStore } from '../stores/userStore'
const emit = defineEmits(['login-success', 'close'])
const userStore = useUserStore() // 使用全局 Store

// === 状态定义 ===
const loginMode = ref<'form' | 'qr'>('form') // 登录模式：表单或二维码
const tabType = ref<'password' | 'phone'>('password') // 表单类型：密码或手机
const isLoading = ref(false)
const errorMsg = ref('')
const rememberMe = ref(false)

// 二维码相关状态
const qrImg = ref('') // 二维码图片 Base64
const qrStatus = ref('') // 用于显示状态提示，如“二维码已过期”

let qrCheckTimer: number | null = null

// === 核心：获取二维码并启动轮询 ===
const initQrLogin = async () => {
  // 清除旧定时器
  if (qrCheckTimer) {
    clearInterval(qrCheckTimer)
    qrCheckTimer = null
  }

  qrStatus.value = '正在加载二维码...'

  try {
    // 1. 获取 Key
    const keyRes = await window.api.login_qr_key({}) as { body?:  LoginQrKey}
    const key = keyRes.body?.data.unikey
    if (!key) return

    // 2. 根据 Key 生成二维码
    const createRes = await window.api.login_qr_create({
      key,
      qrimg: true
    }) as { body?: LoginQrCreate }

    qrImg.value = createRes.body?.data.qrimg ?? ''
    qrStatus.value = '请使用 App 扫码'

    // 3. 开始轮询检测状态
    qrCheckTimer = window.setInterval(async () => {
      const checkRes = await window.api.login_qr_check({ key }) as { body?: LoginQrCheck }
      const code = checkRes.body?.code

      // === 状态码处理 ===

      // 800: 二维码过期
      if (code === 800) {
        clearInterval(qrCheckTimer!)
        qrStatus.value = '二维码已过期，正在刷新...'
        // 重新生成二维码
        initQrLogin()
      }

      // 801: 等待扫码 (无操作)

      // 802: 待确认 (通常是扫码了但在手机上没点确认)
      if (code === 802) {
        qrStatus.value = '扫描成功！请在手机上确认'
      }

      // 803: 登录成功
      if (code === 803) {
        clearInterval(qrCheckTimer!)
        qrCheckTimer = null

        const cookie = checkRes.body?.cookie || ''
        console.log('登录成功，Cookie:', cookie)

        // === 保存到全局状态 & 持久化 ===
        userStore.setLoginData(cookie)

        // 通知父组件
        emit('login-success')
      }
    }, 2000) // 建议每 2-3 秒轮询一次，避免过于频繁

  } catch (err) {
    console.error('二维码初始化错误', err)
    qrStatus.value = '加载二维码失败'
  }
}

// 监听模式切换，切到 QR 模式时才加载二维码
watch(loginMode, (newVal) => {
  if (newVal === 'qr') {
    initQrLogin()
  } else {
    if (qrCheckTimer) clearInterval(qrCheckTimer)
  }
})

// 组件卸载时清理定时器
onUnmounted(() => {
  if (qrCheckTimer) clearInterval(qrCheckTimer)
  if (timerId) clearInterval(timerId)
})

// 表单数据
const creds = reactive({ username: '', password: '' })
const phoneData = reactive({ phone: '', code: '' })
const verificationCodeTimer = ref(0)
let timerId: any = null

const handleClose = () => emit('close')

const toggleLoginMode = () => {
  errorMsg.value = ''
  loginMode.value = loginMode.value === 'form' ? 'qr' : 'form'
}

const sendCode = () => {
  if (!phoneData.phone) {
    errorMsg.value = '请输入手机号码'
    return
  }
  if (verificationCodeTimer.value > 0) return
  verificationCodeTimer.value = 60
  timerId = setInterval(() => {
    verificationCodeTimer.value--
    if (verificationCodeTimer.value <= 0) clearInterval(timerId)
  }, 1000)
}

const handleLogin = () => {
  errorMsg.value = ''
  // ... (保留原有的表单验证逻辑)

  isLoading.value = true

  // 模拟普通登录请求
  setTimeout(() => {
    isLoading.value = false
    // 假设这是接口返回的 cookie
    const mockCookie = 'MUSIC_U=xxxx; __csrf=yyyy;'

    // === 调用 Store 保存 ===
    userStore.setLoginData(mockCookie)

    emit('login-success')
  }, 1500)
}
</script>

<template>
  <div class="login-overlay" @click.self="handleClose">

    <!-- 增加一个翻转容器，用于卡片翻转动画 -->
    <div class="card-perspective">
      <div class="login-card" :class="{ 'is-flipped': loginMode === 'qr' }">

        <!-- === 正面：表单登录 === -->
        <div class="card-face face-front">
          <!-- 顶部操作栏 -->
          <div class="card-top-bar">
            <!-- 切换到二维码 -->
            <button class="icon-btn qr-btn" @click="toggleLoginMode" title="扫码登录">
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="7" height="7"></rect><rect x="14" y="3" width="7" height="7"></rect><rect x="14" y="14" width="7" height="7"></rect><rect x="3" y="14" width="7" height="7"></rect></svg>
            </button>
            <!-- 关闭 -->
            <button class="icon-btn close-btn" @click="handleClose" title="关闭">
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
            </button>
          </div>

          <div class="header-section">
            <h1>欢迎回来</h1>
            <p class="subtitle">请输入您的登录详情</p>
          </div>

          <!-- 现代胶囊切换 Tab -->
          <div class="segmented-control">
            <div class="segment-bg" :style="{ transform: tabType === 'password' ? 'translateX(0)' : 'translateX(100%)' }"></div>
            <button
              type="button"
              :class="{ active: tabType === 'password' }"
              @click="tabType = 'password'"
            >密码登录</button>
            <button
              type="button"
              :class="{ active: tabType === 'phone' }"
              @click="tabType = 'phone'"
            >验证码登录</button>
          </div>

          <form @submit.prevent="handleLogin" class="main-form">
            <!-- 错误提示 -->
            <transition name="fade">
              <div v-if="errorMsg" class="error-msg">{{ errorMsg }}</div>
            </transition>

            <!-- 密码输入组 -->
            <div v-if="tabType === 'password'" class="input-group fade-in-up">
              <div class="input-field">
                <input type="text" v-model="creds.username" placeholder=" " required />
                <label>邮箱</label>
                <div class="icon-suffix">
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"></path><circle cx="12" cy="7" r="4"></circle></svg>
                </div>
              </div>
              <div class="input-field">
                <input type="password" v-model="creds.password" placeholder=" " required />
                <label>密码</label>
                <div class="icon-suffix">
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"></rect><path d="M7 11V7a5 5 0 0 1 10 0v4"></path></svg>
                </div>
              </div>
            </div>

            <!-- 手机输入组 -->
            <div v-else class="input-group fade-in-up">
              <div class="input-field">
                <input type="tel" v-model="phoneData.phone" placeholder=" " required />
                <label>手机号码</label>
                <div class="icon-suffix">
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7A2 2 0 0 1 22 16.92z"></path></svg>
                </div>
              </div>
              <div class="input-field">
                <input type="text" v-model="phoneData.code" placeholder=" " required style="padding-right: 90px;" />
                <label>验证码</label>
                <button type="button" class="verify-btn" :disabled="verificationCodeTimer > 0" @click="sendCode">
                  {{ verificationCodeTimer > 0 ? `${verificationCodeTimer}秒` : '获取验证码' }}
                </button>
              </div>
            </div>

            <!-- 底部辅助 -->
            <div class="form-actions">
              <label class="custom-checkbox">
                <input type="checkbox" v-model="rememberMe">
                <span class="checkmark"></span>
                <span>保持登录状态</span>
              </label>
              <a href="#" class="forgot-link">忘记密码？</a>
            </div>

            <!-- 提交按钮 -->
            <button class="submit-btn" :disabled="isLoading">
              <span v-if="!isLoading">登录</span>
              <div v-else class="spinner"></div>
            </button>
          </form>

        </div>

        <!-- === 背面：二维码登录 === -->
        <div class="card-face face-back">
          <div class="card-top-bar">
            <!-- 返回电脑登录 -->
            <button class="icon-btn pc-btn" @click="toggleLoginMode" title="密码登录">
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="2" y="3" width="20" height="14" rx="2" ry="2"></rect><line x1="8" y1="21" x2="16" y2="21"></line><line x1="12" y1="17" x2="12" y2="21"></line></svg>
            </button>
            <button class="icon-btn close-btn" @click="handleClose">
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
            </button>
          </div>

          <div class="qr-content">
            <h2>扫码登录</h2>
            <div class="qr-box">
              <img v-if="qrImg" :src="qrImg" alt="二维码" />
              <div v-else class="spinner" style="margin: 50px auto; border-top-color: #000;"></div>
            </div>
            <p style="font-weight: 600; color: #000; margin-bottom: 8px;">{{ qrStatus }}</p>
            <p>请打开 <b>移动端 App</b> 扫码即可快速登录。</p>
          </div>
        </div>

      </div>
    </div>
  </div>
</template>

<style scoped>
@import url('https://fonts.googleapis.com/css2?family=Plus+Jakarta+Sans:wght@400;500;600;700&display=swap');

* { box-sizing: border-box; }

/* 基础变量 - 纯黑白极简 */
.login-overlay {
  --bg-overlay: rgba(220, 220, 225, 0.4);
  --card-bg: rgba(255, 255, 255, 0.8);
  --text-main: #000000;
  --text-sub: #666666;
  --accent: #000000;
  --border: #e0e0e0;
  --input-bg: #f4f4f5;
  --input-focus-bg: #ffffff;

  position: fixed; inset: 0; z-index: 9999;
  display: flex; align-items: center; justify-content: center;
  background-color: var(--bg-overlay);
  backdrop-filter: blur(12px);
  font-family: 'Plus Jakarta Sans', sans-serif;
  color: var(--text-main);
}

/* 3D 翻转容器 */
.card-perspective {
  perspective: 1000px;
  width: 100%; display: flex; justify-content: center;
}

.login-card {
  width: 400px;
  min-height: 540px; /* 固定高度保证翻转时不跳动 */
  position: relative;
  transform-style: preserve-3d;
  transition: transform 0.6s cubic-bezier(0.4, 0, 0.2, 1);
  border-radius: 24px;
  box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.1);
}

.login-card.is-flipped {
  transform: rotateY(180deg);
}

/* 卡片正反面通用 */
.card-face {
  position: absolute; inset: 0;
  backface-visibility: hidden;
  background: var(--card-bg);
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
  border: 1px solid rgba(255, 255, 255, 0.5);
  border-radius: 24px;
  padding: 32px;
  display: flex; flex-direction: column;
}

.face-back { transform: rotateY(180deg); text-align: center; }

/* 顶部按钮栏 */
.card-top-bar {
  display: flex; justify-content: space-between; margin-bottom: 24px;
}
.icon-btn {
  width: 36px; height: 36px; border-radius: 12px;
  border: none; background: transparent; color: #888;
  display: flex; align-items: center; justify-content: center;
  cursor: pointer; transition: all 0.2s;
}
.icon-btn:hover { background: rgba(0,0,0,0.05); color: #000; transform: scale(1.05); }

/* 标题区 */
.header-section { margin-bottom: 24px; text-align: center; }
h1 { font-size: 24px; font-weight: 700; margin: 0 0 6px; letter-spacing: -0.5px; }
.subtitle { font-size: 14px; color: var(--text-sub); margin: 0; }

/* === 胶囊切换器 (Segmented Control) === */
.segmented-control {
  position: relative;
  display: flex;
  background: #f0f0f2;
  border-radius: 14px;
  padding: 4px;
  margin-bottom: 28px;
}
.segment-bg {
  position: absolute; top: 4px; left: 4px; bottom: 4px;
  width: calc(50% - 4px);
  background: #fff;
  border-radius: 10px;
  box-shadow: 0 2px 8px rgba(0,0,0,0.08);
  transition: transform 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  z-index: 1;
}
.segmented-control button {
  flex: 1;
  position: relative; z-index: 2;
  border: none; background: transparent;
  font-size: 14px; font-weight: 600; color: #888;
  padding: 8px; cursor: pointer;
  transition: color 0.3s;
}
.segmented-control button.active { color: #000; }

/* === 输入框风格 (Floating Label 风格) === */
.input-group { display: flex; flex-direction: column; gap: 16px; margin-bottom: 20px; }
.input-field { position: relative; }

.input-field input {
  width: 100%; height: 50px;
  background: var(--input-bg);
  border: 1px solid transparent;
  border-radius: 14px;
  padding: 20px 16px 6px; /* 为 Label 留出空间 */
  font-size: 15px; font-weight: 600; color: #000;
  outline: none; transition: all 0.2s;
}

.input-field input:focus {
  background: var(--input-focus-bg);
  border-color: #000;
  box-shadow: 0 4px 12px rgba(0,0,0,0.05);
}

/* Floating Label 逻辑 */
.input-field label {
  position: absolute; left: 16px; top: 15px;
  font-size: 14px; color: #888; font-weight: 500;
  pointer-events: none; transition: all 0.2s;
}
.input-field input:focus ~ label,
.input-field input:not(:placeholder-shown) ~ label {
  top: 7px; font-size: 11px; color: #000; font-weight: 700;
}

/* 右侧图标 */
.icon-suffix {
  position: absolute; right: 16px; top: 50%; transform: translateY(-50%);
  color: #aaa; pointer-events: none; transition: color 0.2s;
}
.input-field input:focus ~ .icon-suffix { color: #000; }

/* 验证码按钮 */
.verify-btn {
  position: absolute; right: 8px; top: 8px; bottom: 8px;
  background: #000; color: #fff;
  border: none; border-radius: 8px;
  font-size: 12px; font-weight: 600; padding: 0 12px;
  cursor: pointer; transition: opacity 0.2s;
}
.verify-btn:disabled { background: #ccc; cursor: not-allowed; }
.verify-btn:hover:not(:disabled) { opacity: 0.8; }

/* 辅助行 */
.form-actions {
  display: flex; justify-content: space-between; align-items: center;
  margin-bottom: 28px; font-size: 13px;
}
.forgot-link { color: #555; text-decoration: none; font-weight: 600; transition: color 0.2s; }
.forgot-link:hover { color: #000; text-decoration: underline; }

/* 黑色 Checkbox */
.custom-checkbox {
  display: flex; align-items: center; gap: 8px; cursor: pointer; color: #555; font-weight: 500;
}
.custom-checkbox input { display: none; }
.checkmark {
  width: 18px; height: 18px; border: 2px solid #ddd; border-radius: 5px;
  position: relative; transition: all 0.2s; background: #fff;
}
.custom-checkbox input:checked ~ .checkmark { background: #000; border-color: #000; }
.checkmark::after {
  content: ''; position: absolute; left: 5px; top: 2px; width: 4px; height: 9px;
  border: solid white; border-width: 0 2px 2px 0; transform: rotate(45deg); opacity: 0;
}
.custom-checkbox input:checked ~ .checkmark::after { opacity: 1; }

/* 提交按钮 */
.submit-btn {
  width: 100%; height: 52px;
  background: #000; color: #fff;
  border: none; border-radius: 14px;
  font-size: 16px; font-weight: 700;
  cursor: pointer; transition: transform 0.1s, background 0.2s;
  display: flex; align-items: center; justify-content: center;
}
.submit-btn:hover { background: #222; transform: translateY(-1px); box-shadow: 0 10px 20px rgba(0,0,0,0.1); }
.submit-btn:active { transform: translateY(1px); }
.submit-btn:disabled { background: #555; cursor: not-allowed; transform: none; }

/* === 二维码面样式 === */
.qr-content {
  flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
}
.qr-content h2 { font-size: 22px; margin-bottom: 20px; }
.qr-box {
  padding: 10px; background: #fff; border-radius: 16px;
  box-shadow: 0 10px 30px rgba(0,0,0,0.08); margin-bottom: 20px;
}
.qr-box img { display: block; width: 180px; height: 180px; }
.qr-content p { color: #666; width: 80%; line-height: 1.5; font-size: 14px; }

/* 错误消息 */
.error-msg {
  color: #d32f2f; font-size: 13px; font-weight: 600;
  background: rgba(211, 47, 47, 0.05); padding: 8px; border-radius: 8px;
  text-align: center; margin-bottom: 12px;
}

/* 动画工具类 */
.fade-enter-active, .fade-leave-active { transition: opacity 0.2s; }
.fade-enter-from, .fade-leave-to { opacity: 0; }

.fade-in-up { animation: fadeInUp 0.4s cubic-bezier(0.2, 0.8, 0.2, 1); }
@keyframes fadeInUp { from { opacity: 0; transform: translateY(10px); } to { opacity: 1; transform: translateY(0); } }

.spinner {
  width: 20px; height: 20px; border: 2px solid rgba(255,255,255,0.3);
  border-top-color: #fff; border-radius: 50%;
  animation: spin 0.8s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }
</style>
