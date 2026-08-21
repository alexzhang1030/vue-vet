<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const unused = computed(() => count.value * 2)
</script>
<template>
  <p>{{ count }}</p>
</template>
