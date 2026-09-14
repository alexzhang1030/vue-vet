<script setup lang="ts">
import { reactive } from 'vue'
const raw = [1] as {
  configure?: (value: TemplateStringsArray) => void
  map?: () => number[]
  __v_skip?: boolean
}
raw.configure = function (this: typeof raw) {
  this.__v_skip = true
  this.map = () => [7]
}
raw.configure`change`
const items = reactive(raw)
const { map } = items
map()
</script>

<template>
  <p />
</template>
