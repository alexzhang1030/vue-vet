<script setup lang="ts">
import { reactive, toRefs } from 'vue'
function install(target: object) {
  Object.assign(target, { added: 7 })
}
const state = reactive({ initial: 1 })
const alias = state
install(alias)
void toRefs(state).added.value
</script>

<template>
  <p />
</template>
