<script setup lang="ts">
import { ref, watch } from 'vue'
let source = 0
;[source] = [ref(1)]
watch(source, () => {})
</script>

<template>
  <p />
</template>
