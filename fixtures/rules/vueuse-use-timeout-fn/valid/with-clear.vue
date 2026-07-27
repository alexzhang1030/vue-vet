<script setup lang="ts">
import { onMounted, onBeforeUnmount } from 'vue'

// Unassigned timeout + a clearTimeout elsewhere: quiets useTimeoutFn without
// matching the assigned clear+set debounce recipe.
let unrelated: ReturnType<typeof setTimeout> | undefined
onMounted(() => {
  setTimeout(() => {
    console.log('ready')
  }, 1000)
})
onBeforeUnmount(() => {
  if (unrelated !== undefined) {
    clearTimeout(unrelated)
  }
})
</script>

<template>
  <div />
</template>
