<script setup lang="ts">
import { computed, ref } from 'vue'
const enabled = ref(false)
const count = ref(0)
function load() {
  return String(count.value)
}
const label = computed(() => (enabled.value ? load() : 'off'))
</script>
<template>
  <p>{{ label }}</p>
</template>
