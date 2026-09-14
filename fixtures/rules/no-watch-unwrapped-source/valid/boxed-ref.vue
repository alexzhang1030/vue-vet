<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
const r = ref(0)
const box: { ref?: { value: unknown } } = {}
box.ref = r
box.ref.value = reactive({ n: 1 })
watch(r.value, () => {})
</script>

<template>
  <p />
</template>
