<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const color = computed(() => (count.value > 0 ? 'red' : 'blue'))
</script>
<template>
  <p>{{ count }}</p>
</template>
<style>
.text {
  color: v-bind(color);
}
</style>
