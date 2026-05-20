<script setup lang="ts">
import AppDialog from '@renderer/components/AppDialog.vue'
// const ipcHandle = (): void => window.electron.ipcRenderer.send('ping')
</script>

<template>
  <RouterView />
  <AppDialog />
</template>
