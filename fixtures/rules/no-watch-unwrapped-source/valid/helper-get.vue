<script setup lang="ts">
import { reactive, watch } from 'vue'
const state = reactive({ child: 1 as number | (() => number) })
const helper = {
  get(target: { child: number | (() => number) }) {
    target.child = () => 7
  },
}
helper.get(state)
watch(state.child, () => {})
</script>

<template>
  <p />
</template>
