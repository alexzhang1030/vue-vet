<script setup lang="ts">
import { ref, watchSyncEffect } from 'vue'
const count = ref(0)
watchSyncEffect(() => count.value, async () => { void count.value })
</script>

<template>
  <p />
</template>
