<script setup lang="ts">
import { reactive } from 'vue'
function configure(target: { __v_skip?: boolean; map?: () => number[] }) {
  target.__v_skip = true
  target.map = () => [7]
}
const raw = [1]
configure(true && raw)
const items = reactive(raw)
const { map } = items
map()
</script>

<template>
  <p />
</template>
