<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const flag = ref(true)
const boxed = computed(() => ({ ok: true }))
watch(boxed, (value) => {
  void value
})
flag.value = false
</script>

<template>
  <p />
</template>
