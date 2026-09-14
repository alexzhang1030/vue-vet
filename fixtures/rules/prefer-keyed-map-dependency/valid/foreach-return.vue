<script setup lang="ts">
import { computed, reactive } from 'vue'
const keyed = reactive(new Map([['selected', 1], ['other', 2]]))
const selected = computed(() => keyed.forEach((entry, key) => {
  if (key === 'selected') void entry
}))
void selected
</script>

<template>
  <p />
</template>
