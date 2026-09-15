<script setup lang="ts">
import { ref } from 'vue'
const revision = ref(0)
const visible = ref(false)
const node = ref(null)
</script>
<template>
  <div v-memo="[revision]">
    <span v-if="visible" ref="node">ready</span>
  </div>
</template>
