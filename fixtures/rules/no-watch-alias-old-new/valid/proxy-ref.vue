<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
const source = reactive(ref(0))
function accept(_value: unknown) {}
watch(source, (next, old) => {
  if (next === old) return
  accept(next)
})
</script>

<template>
  <p />
</template>
