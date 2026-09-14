<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
const r = ref(0)
const a = r
a.value = reactive({ n: 1 })
watch(r.value, () => {})
</script>

<template>
  <p />
</template>
