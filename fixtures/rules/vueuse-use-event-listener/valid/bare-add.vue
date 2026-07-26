<script setup lang="ts">
// Module-style long-lived listener without a setup lifecycle hook stays quiet.
window.addEventListener('resize', () => {
  console.log('resized')
})
</script>

<template>
  <div />
</template>
