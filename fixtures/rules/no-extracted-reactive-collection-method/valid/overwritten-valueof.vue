<script setup lang="ts">
import { reactive } from 'vue'
const raw = [1] as { valueOf?: () => number[]; map?: () => number[]; __v_skip?: boolean }
raw.valueOf = function (this: typeof raw) {
  this.__v_skip = true
  this.map = () => [7]
}
raw.valueOf()
const items = reactive(raw)
const { map } = items
map()
</script>

<template>
  <p />
</template>
