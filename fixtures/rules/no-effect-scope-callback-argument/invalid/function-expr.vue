<script setup lang="ts">
import { effectScope, ref } from 'vue'
const n = ref(0)
effectScope(function () {
  n.value++
})
</script>

<template>
  <p />
</template>
