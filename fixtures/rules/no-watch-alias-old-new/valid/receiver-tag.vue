<script setup lang="ts">
import { reactive, watch } from 'vue'
declare function consume(value: unknown): void
const tagged = reactive({
  value: 0,
  tag: function () {
    this.__v_isRef = true
  },
})
tagged.tag``
watch(tagged, (next, old) => {
  if (next === old) return
  consume(next)
})
</script>

<template>
  <p />
</template>
