<script setup lang="ts">
import { reactive, watch } from 'vue'
const readonlyMarker = reactive({ __v_isReadonly: true, n: 0 })
function accept(_value: unknown) {}
watch(readonlyMarker, (next, old) => {
  if (next === old) return
  accept(next)
})
</script>

<template>
  <p />
</template>
