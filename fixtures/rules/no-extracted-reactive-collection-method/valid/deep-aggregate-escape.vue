<script setup lang="ts">
import { reactive } from 'vue'
function configure(target: unknown) {
  const nested = target as { __v_skip?: boolean; map?: () => number[] }[][][][][][][][][]
  const value = nested[0][0][0][0][0][0][0][0][0]
  value.__v_skip = true
  value.map = () => [7]
}
const raw = [1] as { __v_skip?: boolean; map?: () => number[] }
configure[[[[[[[[[raw]]]]]]]]]
const items = reactive(raw)
const { map } = items
map()
</script>

<template>
  <p />
</template>
