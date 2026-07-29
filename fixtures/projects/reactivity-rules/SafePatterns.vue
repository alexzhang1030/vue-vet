<script setup lang="ts">
import { computed, ref, watchEffect } from 'vue'

const enabled = ref(false)
const result = ref(0)
const first = ref('Ada')
const last = ref('Lovelace')
const fullName = computed(() => `${first.value} ${last.value}`)

watchEffect(() => {
  if (!enabled.value) return
  const later = () => result.value
  void later
})
</script>

<template>
  <p>{{ fullName }}</p>
</template>
