<script setup lang="ts">
import { reactive, watch } from 'vue'
const state = reactive({ child: { count: 1 } })
false && watch(state.child, () => {})
state.child = { count: 2 }
</script>

<template>
  <p />
</template>
