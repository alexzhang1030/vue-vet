<script setup lang="ts">
import { reactive, watch } from 'vue'
const taggedLater = reactive({ value: 0 })
taggedLater.__v_isRef = true
function accept(_value: unknown) {}
watch(taggedLater, (next, old) => {
  if (next === old) return
  accept(next)
})
</script>

<template>
  <p />
</template>
