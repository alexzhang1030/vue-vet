<script setup lang="ts">
import { reactive, toRefs } from 'vue'
function install<T>(this: { added?: number }) {
  this.added = 7
}
const state = reactive({ initial: 1, install })
;(state.install<number>)``
const fields = toRefs(state)
void fields.added.value
</script>

<template>
  <p />
</template>
