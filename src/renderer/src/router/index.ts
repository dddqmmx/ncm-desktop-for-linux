import { createRouter, createWebHistory } from 'vue-router'
import MainView from '../views/MainView.vue'
import HomeView from '@renderer/views/HomeView.vue'
import SearchView from '@renderer/views/SearchView.vue'
import PlaylistView from '@renderer/views/PlaylistView.vue'

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    {
      path: '/',
      name: 'main',
      component: MainView,
      children: [
        {
          path: '',
          redirect: 'home'
        },
        {
          path: 'home',
          name: 'home',
          component: HomeView
        },
        {
          path: 'search',
          name: 'search',
          component: SearchView
        },
        {
          path: 'new',
          name: 'new',
          component: HomeView
        },
        {
          path: 'radio',
          name: 'radio',
          component: HomeView
        },
        {
          path: 'playlist/:id',
          name: 'playlist',
          component: PlaylistView,
          props: true
        }
      ]
    }
  ]
})

export default router
