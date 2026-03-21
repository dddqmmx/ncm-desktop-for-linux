import './assets/main.css'

import { createApp } from 'vue'
import App from './App.vue'
import { createPinia } from 'pinia'
import router from './router'
import { useConfigStore } from './stores/configStore'

const pinia = createPinia()
const app = createApp(App)
app.use(pinia)
app.use(router)

const configStore = useConfigStore(pinia)
void configStore.initialize()

app.mount('#app')
