<script setup lang="ts">
import { reactive } from 'vue'
const raw = [1] as { map?: () => number[]; __v_skip?: boolean }
function configure() {
  alias.__v_skip = true
  alias.map = () => [7]
}
const alias = raw
configure()
const items = reactive(raw)
const { map } = items
map()
</script>

<template>
  <p />
</template>
