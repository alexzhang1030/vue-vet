<script setup>
import { onMounted } from 'vue'

onMounted(() => {
  function render() {
    function later() {
      requestAnimationFrame(render)
    }
  }
  requestAnimationFrame(render)
})
</script>
<template></template>
