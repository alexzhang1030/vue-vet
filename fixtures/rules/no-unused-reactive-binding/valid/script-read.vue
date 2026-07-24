<script setup lang="ts">
import { ref, watchEffect } from 'vue'

const count = ref(0)
watchEffect(() => {
  console.log(count.value)
})
</script>

<template>
  <div />
</template>
