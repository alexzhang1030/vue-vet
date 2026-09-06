<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const count = ref(0)
const doubled = computed(() => count.value * 2)
watch(doubled, (doubled) => {
  if (!doubled)
    return
})
</script>

<template>
  <p>{{ doubled }}</p>
</template>
