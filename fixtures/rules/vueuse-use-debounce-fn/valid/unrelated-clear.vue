<script setup lang="ts">
let timer: ReturnType<typeof setTimeout> | undefined
let other: ReturnType<typeof setTimeout> | undefined
clearTimeout(other)
timer = setTimeout(() => {
  console.log('once')
}, 200)
</script>

<template>
  <div />
</template>
