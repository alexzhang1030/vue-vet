<script setup lang="ts">
import { ref } from 'vue'
const visible = ref(true)
</script>
<template>
  <Transition name="fade">
    <div v-if="visible">Sometimes visible</div>
  </Transition>
</template>
