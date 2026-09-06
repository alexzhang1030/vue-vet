<script setup lang="ts">
import { ref, watch } from 'vue'
const r = ref(0)
function initialize(target: { value: unknown }) {
  target.value = {}
}
initialize(r)
watch(r.value, () => {})
</script>

<template>
  <p />
</template>
