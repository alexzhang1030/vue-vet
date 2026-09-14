<script setup lang="ts">
import { computed, reactive } from 'vue'
const inner = new Map([['selected', 7]])
const keyed = reactive(inner)
const selected = computed(() => keyed.get('selected'))
void selected
</script>

<template>
  <p />
</template>
