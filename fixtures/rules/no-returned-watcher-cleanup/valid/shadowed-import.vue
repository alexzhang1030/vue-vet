<script setup lang="ts">
const watchEffect = (callback: () => unknown) => {
  callback()
}
watchEffect(() => {
  return () => {}
})
</script>
<template>ok</template>
