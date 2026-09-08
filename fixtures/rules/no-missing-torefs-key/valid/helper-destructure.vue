<script setup lang="ts">
import { reactive, toRefs } from 'vue'
function install(target: object) {
  Object.assign(target, { added: 7 })
}
const state = reactive({ initial: 1 })
install(state)
const { added } = toRefs(state)
void added.value
</script>

<template>
  <p />
</template>
