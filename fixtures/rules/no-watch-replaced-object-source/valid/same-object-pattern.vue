<script setup lang="ts">
import { reactive, watch } from 'vue'
const state = reactive({ child: { n: 0 } })
watch(state.child, () => {})
;({ child: state.child } = { child: state.child })
</script>

<template>
  <p />
</template>
