<script setup lang="ts">
import { effectScope, ref } from 'vue'
const 数 = ref(0)
effectScope(() => {
  数.value++
})
</script>

<template>
  <p />
</template>
