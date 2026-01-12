import { UserAccount } from '@renderer/types/userAccount'
import { defineStore } from 'pinia'
import { ref } from 'vue'

export const useUserStore = defineStore('user', () => {
  // === 状态 ===
  const cookie = ref(localStorage.getItem('app_cookie') || '')
  const userInfo = ref<UserAccount | undefined>(undefined)
  const isLoggedIn = ref(!!localStorage.getItem('app_cookie'))
5
  // === Action: 登录成功保存数据 ===
  function setLoginData(newCookie: string) {
    cookie.value = newCookie
    isLoggedIn.value = true
    // 写入 localStorage 实现长期存储
    localStorage.setItem('app_cookie', newCookie)
    // 如果需要设置到浏览器 Cookie (根据你的请求库需求)
    document.cookie = newCookie
  }

  async function getUserAccount(){
    if (isLoggedIn.value) {
      const res = await window.api.user_account({cookie: cookie.value}) as { body?: UserAccount }
      userInfo.value = res.body
    }
  }

  // === Action: 登出 ===
  function logout() {
    cookie.value = ''
    userInfo.value = undefined
    isLoggedIn.value = false

    localStorage.removeItem('app_cookie')
    localStorage.removeItem('app_user_info')
  }

  return {
    cookie,
    userInfo,
    isLoggedIn,
    setLoginData,
    getUserAccount,
    logout
  }
})
