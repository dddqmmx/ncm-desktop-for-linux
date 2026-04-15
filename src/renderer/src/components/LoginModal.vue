<script setup lang="ts">
import { ref, reactive, onUnmounted, watch } from 'vue'
import { useUserStore } from '../stores/userStore'
import type { LoginQrCheck, LoginQrCreate, LoginQrKey } from '../types/loginQr'

const emit = defineEmits(['login-success', 'close'])
const userStore = useUserStore()

// === 状态定义 ===
const loginMode = ref<'form' | 'qr'>('form')
const tabType = ref<'password' | 'phone'>('password')
const isLoading = ref(false)
const errorMsg = ref('')
const rememberMe = ref(false)

// 验证码计时器相关
const verificationCodeTimer = ref(0)
let timerId: ReturnType<typeof setInterval> | null = null
let qrCheckTimer: number | null = null

// 二维码相关状态
const qrImg = ref('')
const qrStatus = ref('')

// === 核心：获取二维码并启动轮询 ===
const initQrLogin = async (): Promise<void> => {
  if (qrCheckTimer) {
    clearInterval(qrCheckTimer)
    qrCheckTimer = null
  }

  qrStatus.value = '正在加载二维码...'

  try {
    const keyRes = (await window.api.login_qr_key({})) as { body?: LoginQrKey }
    const key = keyRes.body?.data.unikey
    if (!key) return

    const createRes = (await window.api.login_qr_create({
      key,
      qrimg: true
    })) as { body?: LoginQrCreate }

    qrImg.value = createRes.body?.data.qrimg ?? ''
    qrStatus.value = '请使用 App 扫码'

    qrCheckTimer = window.setInterval(async () => {
      const checkRes = (await window.api.login_qr_check({ key })) as { body?: LoginQrCheck }
      const code = checkRes.body?.code

      if (code === 800) {
        clearInterval(qrCheckTimer!)
        qrStatus.value = '二维码已过期，正在刷新...'
        initQrLogin()
      }

      if (code === 802) {
        qrStatus.value = '扫描成功！请在手机上确认'
      }

      if (code === 803) {
        clearInterval(qrCheckTimer!)
        qrCheckTimer = null
        const cookie = checkRes.body?.cookie || ''
        userStore.setLoginData(cookie)
        emit('login-success')
      }
    }, 2000)
  } catch (err) {
    console.error('二维码初始化错误', err)
    qrStatus.value = '加载二维码失败'
  }
}

// 监听模式切换
watch(loginMode, (newVal) => {
  if (newVal === 'qr') {
    initQrLogin()
  } else {
    if (qrCheckTimer) clearInterval(qrCheckTimer)
  }
})

// 监听 Tab 切换，清空错误信息
watch(tabType, () => {
  errorMsg.value = ''
})

onUnmounted(() => {
  if (qrCheckTimer) clearInterval(qrCheckTimer)
  if (timerId) clearInterval(timerId)
})

// 表单数据
const creds = reactive({ phone: '', password: '', countrycode: '86' })
const phoneData = reactive({ phone: '', code: '', countrycode: '86' })

const handleClose = (): void => emit('close')

const toggleLoginMode = (): void => {
  errorMsg.value = ''
  loginMode.value = loginMode.value === 'form' ? 'qr' : 'form'
}

const sendCode = async (): Promise<void> => {
  if (!phoneData.phone) {
    errorMsg.value = '请输入手机号码'
    return
  }
  if (verificationCodeTimer.value > 0) return

  errorMsg.value = ''
  try {
    const res = (await window.api.captcha_sent({
      phone: phoneData.phone,
      ctcode: phoneData.countrycode
    })) as { body?: { code: number; message?: string } }

    if (res.body?.code === 200) {
      verificationCodeTimer.value = 60
      timerId = setInterval(() => {
        verificationCodeTimer.value--
        if (verificationCodeTimer.value <= 0 && timerId) clearInterval(timerId)
      }, 1000)
    } else {
      errorMsg.value = res.body?.message || '发送验证码失败'
    }
  } catch (err) {
    console.error('发送验证码错误', err)
    errorMsg.value = '发送验证码请求异常'
  }
}

const handleLogin = async (): Promise<void> => {
  errorMsg.value = ''

  if (tabType.value === 'password') {
    if (!creds.phone || !creds.password) {
      errorMsg.value = '请输入手机号和密码'
      return
    }
  } else {
    if (!phoneData.phone || !phoneData.code) {
      errorMsg.value = '请输入手机号和验证码'
      return
    }
  }

  isLoading.value = true

  try {
    const params =
      tabType.value === 'password'
        ? {
            phone: creds.phone,
            password: creds.password,
            countrycode: creds.countrycode,
            rememberMe: rememberMe.value
          }
        : {
            phone: phoneData.phone,
            captcha: phoneData.code,
            countrycode: phoneData.countrycode,
            rememberMe: rememberMe.value
          }

    const res = (await window.api.login(params)) as {
      body?: { code: number; message?: string }
      cookie?: string | string[]
    }

    if (res.body?.code === 200) {
      let cookie = ''
      if (Array.isArray(res.cookie)) {
        cookie = res.cookie.join('; ')
      } else if (typeof res.cookie === 'string') {
        cookie = res.cookie
      }
      userStore.setLoginData(cookie)
      emit('login-success')
    } else {
      errorMsg.value = res.body?.message || '登录失败，请重试'
    }
  } catch (err) {
    console.error('登录异常', err)
    errorMsg.value = '请求登录接口异常'
  } finally {
    isLoading.value = false
  }
}
</script>

<template>
  <div class="login-overlay" @click.self="handleClose">
    <div class="card-perspective">
      <div class="login-card" :class="{ 'is-flipped': loginMode === 'qr' }">
        <!-- === 正面：表单登录 === -->
        <div class="card-face face-front">
          <div class="card-top-bar">
            <button class="icon-btn qr-btn" title="扫码登录" @click="toggleLoginMode">
              <svg
                width="20"
                height="20"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <rect x="3" y="3" width="7" height="7"></rect>
                <rect x="14" y="3" width="7" height="7"></rect>
                <rect x="14" y="14" width="7" height="7"></rect>
                <rect x="3" y="14" width="7" height="7"></rect>
              </svg>
            </button>
            <button class="icon-btn close-btn" title="关闭" @click="handleClose">
              <svg
                width="20"
                height="20"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <line x1="18" y1="6" x2="6" y2="18"></line>
                <line x1="6" y1="6" x2="18" y2="18"></line>
              </svg>
            </button>
          </div>

          <div class="header-section">
            <h1>欢迎回来</h1>
            <p class="subtitle">请输入您的登录详情</p>
          </div>

          <div class="segmented-control">
            <div
              class="segment-bg"
              :style="{ transform: tabType === 'password' ? 'translateX(0)' : 'translateX(100%)' }"
            ></div>
            <button
              type="button"
              :class="{ active: tabType === 'password' }"
              @click="tabType = 'password'"
            >
              密码登录
            </button>
            <button
              type="button"
              :class="{ active: tabType === 'phone' }"
              @click="tabType = 'phone'"
            >
              验证码登录
            </button>
          </div>

          <form class="main-form" @submit.prevent="handleLogin">
            <div class="error-msg-wrapper">
              <transition name="fade">
                <div v-if="errorMsg" class="error-msg">{{ errorMsg }}</div>
              </transition>
            </div>

            <!-- 密码输入组 -->
            <div v-if="tabType === 'password'" class="input-group fade-in-up">
              <div class="phone-input-container">
                <div class="prefix-selector">
                  <span>+</span>
                  <input v-model="creds.countrycode" type="text" placeholder="86" />
                </div>
                <div class="main-input">
                  <input
                    id="pwd-phone"
                    v-model="creds.phone"
                    class="has-icon"
                    type="tel"
                    placeholder=" "
                    required
                  />
                  <label for="pwd-phone">手机号码</label>
                </div>
                <div class="icon-suffix">
                  <svg
                    width="18"
                    height="18"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                  >
                    <path
                      d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7A2 2 0 0 1 22 16.92z"
                    ></path>
                  </svg>
                </div>
              </div>
              <div class="input-field">
                <input
                  id="pwd-input"
                  v-model="creds.password"
                  class="has-icon"
                  type="password"
                  placeholder=" "
                  required
                />
                <label for="pwd-input">密码</label>
                <div class="icon-suffix">
                  <svg
                    width="18"
                    height="18"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                  >
                    <rect x="3" y="11" width="18" height="11" rx="2" ry="2"></rect>
                    <path d="M7 11V7a5 5 0 0 1 10 0v4"></path>
                  </svg>
                </div>
              </div>
            </div>

            <!-- 手机输入组 -->
            <div v-else class="input-group fade-in-up">
              <div class="phone-input-container">
                <div class="prefix-selector">
                  <span>+</span>
                  <input v-model="phoneData.countrycode" type="text" placeholder="86" />
                </div>
                <div class="main-input">
                  <input
                    id="code-phone"
                    v-model="phoneData.phone"
                    class="has-icon"
                    type="tel"
                    placeholder=" "
                    required
                  />
                  <label for="code-phone">手机号码</label>
                </div>
                <div class="icon-suffix">
                  <svg
                    width="18"
                    height="18"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                  >
                    <path
                      d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7A2 2 0 0 1 22 16.92z"
                    ></path>
                  </svg>
                </div>
              </div>
              <div class="input-field">
                <input
                  id="code-input"
                  v-model="phoneData.code"
                  class="has-btn"
                  type="text"
                  placeholder=" "
                  required
                />
                <label for="code-input">验证码</label>
                <button
                  type="button"
                  class="verify-btn"
                  :disabled="verificationCodeTimer > 0"
                  @click="sendCode"
                >
                  {{
                    verificationCodeTimer > 0 ? `${verificationCodeTimer}秒后获取` : '获取验证码'
                  }}
                </button>
              </div>
            </div>

            <!-- 底部辅助 -->
            <div class="form-actions">
              <label class="custom-checkbox">
                <input v-model="rememberMe" type="checkbox" />
                <span class="checkmark"></span>
                <span>保持登录状态</span>
              </label>
              <a href="#" class="forgot-link">忘记密码？</a>
            </div>

            <button class="submit-btn" :disabled="isLoading">
              <span v-if="!isLoading">登录</span>
              <div v-else class="spinner"></div>
            </button>
          </form>
        </div>

        <!-- === 背面：二维码登录 === -->
        <div class="card-face face-back">
          <div class="card-top-bar">
            <button class="icon-btn pc-btn" title="密码登录" @click="toggleLoginMode">
              <svg
                width="20"
                height="20"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <rect x="2" y="3" width="20" height="14" rx="2" ry="2"></rect>
                <line x1="8" y1="21" x2="16" y2="21"></line>
                <line x1="12" y1="17" x2="12" y2="21"></line>
              </svg>
            </button>
            <button class="icon-btn close-btn" @click="handleClose">
              <svg
                width="20"
                height="20"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <line x1="18" y1="6" x2="6" y2="18"></line>
                <line x1="6" y1="6" x2="18" y2="18"></line>
              </svg>
            </button>
          </div>

          <div class="qr-content">
            <h2>扫码登录</h2>
            <div class="qr-box">
              <img v-if="qrImg" :src="qrImg" alt="二维码" />
              <div v-else class="spinner" style="border-top-color: #000"></div>
            </div>
            <p class="qr-status-text">{{ qrStatus }}</p>
            <p class="qr-desc">请打开 <b>移动端 App</b> 扫码即可快速登录。</p>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
* {
  box-sizing: border-box;
}

/* 基础变量 */
.login-overlay {
  --bg-overlay: rgba(220, 220, 225, 0.4);
  --card-bg: rgba(255, 255, 255, 0.9);
  --text-main: #000000;
  --text-sub: #666666;
  --accent: #000000;
  --border: #e0e0e0;
  --input-bg: #f4f4f5;
  --input-focus-bg: #ffffff;

  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  background-color: var(--bg-overlay);
  backdrop-filter: blur(12px);
  font-family:
    'SF Pro Text',
    'SF Pro Display',
    -apple-system,
    BlinkMacSystemFont,
    'Segoe UI',
    sans-serif;
  color: var(--text-main);
}

/* 3D 翻转容器 */
.card-perspective {
  perspective: 1200px;
  width: 100%;
  display: flex;
  justify-content: center;
}

.login-card {
  width: 400px;
  min-height: 560px; /* 增加一点高度防止内部抖动 */
  position: relative;
  transform-style: preserve-3d;
  transition: transform 0.6s cubic-bezier(0.4, 0, 0.2, 1);
  border-radius: 24px;
  box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.15);
}

.login-card.is-flipped {
  transform: rotateY(180deg);
}

.card-face {
  position: absolute;
  inset: 0;
  backface-visibility: hidden;
  background: var(--card-bg);
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
  border: 1px solid rgba(255, 255, 255, 0.8);
  border-radius: 24px;
  padding: 32px;
  display: flex;
  flex-direction: column;
}

.face-back {
  transform: rotateY(180deg);
  text-align: center;
}

/* 顶部操作栏 */
.card-top-bar {
  display: flex;
  justify-content: space-between;
  margin-bottom: 20px;
}

.icon-btn {
  width: 36px;
  height: 36px;
  border-radius: 12px;
  border: none;
  background: transparent;
  color: #888;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: all 0.2s;
}

.icon-btn:hover {
  background: rgba(0, 0, 0, 0.05);
  color: #000;
  transform: scale(1.05);
}

/* 标题 */
.header-section {
  margin-bottom: 24px;
  text-align: center;
}
h1 {
  font-size: 24px;
  font-weight: 700;
  margin: 0 0 6px;
  letter-spacing: -0.5px;
}
.subtitle {
  font-size: 14px;
  color: var(--text-sub);
  margin: 0;
}

/* 胶囊切换 Tab */
.segmented-control {
  position: relative;
  display: flex;
  background: #f0f0f2;
  border-radius: 14px;
  padding: 4px;
}
.segment-bg {
  position: absolute;
  top: 4px;
  left: 4px;
  bottom: 4px;
  width: calc(50% - 4px);
  background: #fff;
  border-radius: 10px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08);
  transition: transform 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  z-index: 1;
}
.segmented-control button {
  flex: 1;
  position: relative;
  z-index: 2;
  border: none;
  background: transparent;
  font-size: 14px;
  font-weight: 600;
  color: #888;
  padding: 10px;
  cursor: pointer;
  transition: color 0.3s;
}
.segmented-control button.active {
  color: #000;
}

/* === 输入框核心系统 === */
.input-group {
  display: flex;
  flex-direction: column;
  gap: 16px;
  margin-bottom: 24px;
}

.input-field,
.phone-input-container {
  position: relative;
  background: var(--input-bg);
  border-radius: 14px;
  border: 1px solid transparent;
  transition: all 0.2s;
  display: flex;
}

.input-field:focus-within,
.phone-input-container:focus-within {
  background: var(--input-focus-bg);
  border-color: #000;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.05);
}

/* 基础 Input 样式 */
.input-field input,
.main-input input {
  width: 100%;
  height: 54px;
  background: transparent;
  border: none;
  padding: 22px 16px 6px; /* 顶部留白给悬浮Label */
  font-size: 15px;
  font-weight: 600;
  color: #000;
  outline: none;
}

/* 预留右侧空间防止文字被盖住 */
input.has-icon {
  padding-right: 42px;
}
input.has-btn {
  padding-right: 110px;
}

/* 悬浮标签 Floating Label 核心逻辑 */
.input-field label,
.main-input label {
  position: absolute;
  left: 16px;
  top: 17px;
  font-size: 14px;
  color: #888;
  font-weight: 500;
  pointer-events: none;
  transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
}

.input-field input:focus ~ label,
.input-field input:not(:placeholder-shown) ~ label,
.main-input input:focus ~ label,
.main-input input:not(:placeholder-shown) ~ label {
  top: 8px;
  font-size: 11px;
  color: #000;
  font-weight: 700;
}

/* 手机号特定区域 */
.phone-input-container {
  align-items: stretch;
}
.prefix-selector {
  display: flex;
  align-items: center;
  padding-left: 16px;
  color: #888;
  font-weight: 600;
  font-size: 15px;
}
.prefix-selector span {
  margin-right: 2px;
}
.prefix-selector input {
  width: 28px;
  background: transparent;
  border: none;
  padding: 0;
  font-size: 15px;
  color: #000;
  font-weight: 600;
  outline: none;
  text-align: left;
}
.prefix-selector::after {
  content: '';
  display: block;
  width: 1px;
  height: 20px;
  background: #ddd;
  margin-left: 10px;
}
.main-input {
  flex: 1;
  position: relative;
}

/* 图标后缀 */
.icon-suffix {
  position: absolute;
  right: 16px;
  top: 50%;
  transform: translateY(-50%);
  color: #aaa;
  pointer-events: none;
  transition: color 0.2s;
  display: flex;
  align-items: center;
}
.input-field:focus-within .icon-suffix,
.phone-input-container:focus-within .icon-suffix {
  color: #000;
}

/* 获取验证码按钮 */
.verify-btn {
  position: absolute;
  right: 8px;
  top: 8px;
  bottom: 8px;
  background: #000;
  color: #fff;
  border: none;
  border-radius: 10px;
  font-size: 12px;
  font-weight: 600;
  padding: 0 14px;
  cursor: pointer;
  transition: all 0.2s;
}
.verify-btn:disabled {
  background: #e0e0e0;
  color: #999;
  cursor: not-allowed;
}
.verify-btn:hover:not(:disabled) {
  background: #333;
}

/* 错误消息容器（占位防跳动） */
.error-msg-wrapper {
  margin-top: 6px;
  min-height: 20px;
  margin-bottom: 6px;
}
.error-msg {
  color: #d32f2f;
  font-size: 13px;
  font-weight: 600;
  background: rgba(211, 47, 47, 0.08);
  padding: 10px;
  border-radius: 10px;
  text-align: center;
}

/* 底部操作 */
.form-actions {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 28px;
  font-size: 13px;
}
.forgot-link {
  color: #555;
  text-decoration: none;
  font-weight: 600;
  transition: color 0.2s;
}
.forgot-link:hover {
  color: #000;
}

/* 自定义 Checkbox */
.custom-checkbox {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  color: #555;
  font-weight: 500;
}
.custom-checkbox input {
  display: none;
}
.checkmark {
  width: 18px;
  height: 18px;
  border: 2px solid #ccc;
  border-radius: 6px;
  position: relative;
  transition: all 0.2s;
  background: #fff;
}
.custom-checkbox input:checked ~ .checkmark {
  background: #000;
  border-color: #000;
}
.checkmark::after {
  content: '';
  position: absolute;
  left: 5px;
  top: 2px;
  width: 4px;
  height: 9px;
  border: solid white;
  border-width: 0 2px 2px 0;
  transform: rotate(45deg);
  opacity: 0;
}
.custom-checkbox input:checked ~ .checkmark::after {
  opacity: 1;
}

/* 提交按钮 */
.submit-btn {
  width: 100%;
  height: 54px;
  background: #000;
  color: #fff;
  border: none;
  border-radius: 14px;
  font-size: 16px;
  font-weight: 700;
  cursor: pointer;
  transition:
    transform 0.1s,
    background 0.2s,
    box-shadow 0.2s;
  display: flex;
  align-items: center;
  justify-content: center;
}
.submit-btn:hover:not(:disabled) {
  background: #222;
  transform: translateY(-1px);
  box-shadow: 0 8px 16px rgba(0, 0, 0, 0.15);
}
.submit-btn:active:not(:disabled) {
  transform: translateY(1px);
}
.submit-btn:disabled {
  background: #ccc;
  cursor: not-allowed;
}

/* === 二维码区域 === */
.qr-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  margin-top: -20px;
}
.qr-content h2 {
  font-size: 22px;
  margin-bottom: 24px;
}
.qr-box {
  width: 220px;
  height: 220px;
  background: #fff;
  border-radius: 20px;
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.08);
  margin-bottom: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  border: 1px solid #eee;
}
.qr-box img {
  width: 190px;
  height: 190px;
  object-fit: contain;
}
.qr-status-text {
  font-weight: 600;
  color: #000;
  margin-bottom: 8px;
  font-size: 15px;
}
.qr-desc {
  color: #666;
  font-size: 13px;
}

/* 动画工具 */
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.3s;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.fade-in-up {
  animation: fadeInUp 0.4s cubic-bezier(0.2, 0.8, 0.2, 1);
}
@keyframes fadeInUp {
  from {
    opacity: 0;
    transform: translateY(10px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.spinner {
  width: 22px;
  height: 22px;
  border: 2px solid rgba(255, 255, 255, 0.3);
  border-top-color: #fff;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
