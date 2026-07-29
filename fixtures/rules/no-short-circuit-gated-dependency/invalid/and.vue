<script setup lang="ts">
import { computed, ref } from 'vue'

const enabled = ref(false)
const count = ref(0)
const label = computed(() => enabled.value && String(count.value))
</script>

<template>
  <p>{{ label }}</p>
</template>
