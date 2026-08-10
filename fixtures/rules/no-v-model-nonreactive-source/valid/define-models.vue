<script setup lang="ts">
// Vue Macros multi-model form (vitesse / unplugin-vue-macros).
const { modelValue } = defineModels<{
  modelValue: string
}>()
</script>

<template>
  <input v-model="modelValue" />
</template>
