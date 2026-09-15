<script setup lang="ts">
import { computed, reactive } from 'vue'
const keyed = reactive(new Map([['selected', 1], ['other', 2]]))
const selected = computed(() => keyed.get('selected'))
void selected
</script>

<template>
  <p />
</template>
