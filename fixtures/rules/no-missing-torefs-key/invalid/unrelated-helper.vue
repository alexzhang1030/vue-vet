<script setup lang="ts">
import { reactive, toRefs } from 'vue'
function install(target: object) {
  Object.assign(target, { added: 7 })
}
const state = reactive({ initial: 1 })
const other = reactive({ initial: 1 })
install(other)
void toRefs(state).missing.value
</script>

<template>
  <p />
</template>
