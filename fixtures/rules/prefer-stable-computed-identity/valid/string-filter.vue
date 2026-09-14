<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const items = ref(['a'])
const selected = computed(() => items.value.filter((n) => n < 'm'))
watch(selected, (value) => {
  void value
}, { flush: 'sync' })
items.value = ['b']
</script>

<template>
  <p />
</template>
