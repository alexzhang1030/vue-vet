<script setup lang="ts">
import { ref, watchSyncEffect } from 'vue'
import { computedWithControl } from '@vueuse/shared'
const revision = ref(0)
const source = ref(1)
const value = computedWithControl(revision, () => source.value)
watchSyncEffect(() => {
  void value.value
})
source.value = 2
</script>

<template>
  <p />
</template>
