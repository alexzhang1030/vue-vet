<script setup lang="ts">
import { reactive } from 'vue'
function run() {
  const items = reactive(new Map())
  const { get } = items
  return 7
  get('key')
}
run()
</script>

<template>
  <p />
</template>
