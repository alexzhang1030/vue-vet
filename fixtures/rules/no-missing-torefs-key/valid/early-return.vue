<script setup lang="ts">
import { reactive, toRefs } from 'vue'
function run() {
  const bag = toRefs(reactive({ count: 1 }))
  if (!bag.missing) return
  void bag.missing.value
}
run()
</script>

<template>
  <p />
</template>
