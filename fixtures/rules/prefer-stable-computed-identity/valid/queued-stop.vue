<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const items = ref([1, 2])
const doubled = computed(() => items.value.map((n) => n * 2))
const stop = watch(doubled, (value) => {
  void value
}, { flush: 'pre' })
items.value = [1, 2]
stop()
</script>

<template>
  <p />
</template>
