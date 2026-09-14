<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const items = ref([NaN])
const copy = computed(() => items.value.map((n) => n))
watch(copy, (value) => {
  void value
})
items.value = [NaN]
</script>

<template>
  <p />
</template>
