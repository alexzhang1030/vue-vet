<script setup lang="ts">
import { reactive } from 'vue'
function tag(value: object) {
  void value
}
const map = reactive(new Map([['a', 1]]))
tag(map)
const { get } = map
get('a')
</script>

<template>
  <p />
</template>
