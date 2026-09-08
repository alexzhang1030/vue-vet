<script setup lang="ts">
import { reactive, watch } from 'vue'
const tagged = reactive({ __v_isRef: true, value: 0 })
function accept(_value: unknown) {}
watch(tagged, (next, old) => {
  if (next === old) return
  accept(next)
})
</script>

<template>
  <p />
</template>
