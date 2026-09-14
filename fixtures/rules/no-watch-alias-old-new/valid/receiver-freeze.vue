<script setup lang="ts">
import { reactive, watch } from 'vue'
declare function consume(value: unknown): void
const raw = {
  n: 0,
  lock: function () {
    Object.freeze(this)
  },
}
raw.lock()
const state = reactive(raw)
watch(state, (next, old) => {
  if (next === old) return
  consume(next)
})
</script>

<template>
  <p />
</template>
