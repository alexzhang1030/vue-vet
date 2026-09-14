<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
const r = ref(0)
;({ x: r.value } = { x: reactive({ n: 1 }) })
watch(r.value, () => {})
</script>

<template>
  <p />
</template>
