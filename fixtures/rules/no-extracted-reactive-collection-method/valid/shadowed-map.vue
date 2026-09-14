<script setup lang="ts">
import { reactive } from 'vue'
class Map {
  get(_key: string) {
    return 1
  }
}
const map = reactive(new Map())
const { get } = map
get('a')
</script>

<template>
  <p />
</template>
