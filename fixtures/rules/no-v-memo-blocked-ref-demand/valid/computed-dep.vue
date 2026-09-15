<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from 'vue'
const visible = ref(false)
const shown = computed(() => visible.value)
const node = ref(null)
onMounted(async () => {
  visible.value = true
  await nextTick()
  node.value.textContent
})
</script>
<template>
  <div v-memo="[shown]">
    <span v-if="visible" ref="node">ready</span>
  </div>
</template>
