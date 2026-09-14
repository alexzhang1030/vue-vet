<script setup lang="ts">
import { ref, toRef } from 'vue'
const count = ref(0)
const same = toRef(count, 'value')
same.value = 1
</script>

<template>
  <p />
</template>
