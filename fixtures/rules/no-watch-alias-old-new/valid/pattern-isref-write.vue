<script setup lang="ts">
import { reactive, watch } from 'vue'
const tagged = reactive({ value: 0 })
;({ x: tagged.__v_isRef } = { x: true })
function accept(_value: unknown) {}
watch(tagged, (next, old) => {
  if (next === old) return
  accept(next)
})
</script>

<template>
  <p />
</template>
