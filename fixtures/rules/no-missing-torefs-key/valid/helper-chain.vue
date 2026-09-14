<script setup lang="ts">
import { reactive, toRefs } from 'vue'
function install(target: object) {
  Object.assign(target, { added: 7 })
}
const state = reactive({ initial: 1 })
install(state)
void toRefs(state).added.value
</script>

<template>
  <p />
</template>
