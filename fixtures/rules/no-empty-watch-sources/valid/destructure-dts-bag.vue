<script setup lang="ts">
import { watch, type Ref } from 'vue'

declare function useElementSize(): {
  width: Ref<number>
  height: Ref<number>
  stop: () => void
}

const { width: hostWidth, height: hostHeight } = useElementSize()
watch([hostWidth, hostHeight], () => {})
</script>

<template>
  <p>{{ hostWidth }} × {{ hostHeight }}</p>
</template>
