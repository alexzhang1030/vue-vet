<script setup lang="ts">
import { reactive } from 'vue'
const raw = [1] as { __v_skip?: boolean; map?: () => number[] }
;({ skip: raw.__v_skip, map: raw.map } = { skip: true, map: () => [7] })
const items = reactive(raw)
const { map } = items
map()
</script>

<template>
  <p />
</template>
