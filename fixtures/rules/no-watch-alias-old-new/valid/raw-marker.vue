<script setup lang="ts">
import { reactive, watch } from 'vue'
const rawMarker = reactive({ __v_raw: { n: 0 }, n: 0 })
function accept(_value: unknown) {}
watch(rawMarker, (next, old) => {
  if (next === old) return
  accept(next)
})
</script>

<template>
  <p />
</template>
