<script setup lang="ts">
import { reactive, ref, toRefs } from 'vue'
const refs = toRefs(reactive({ count: 1 }))
refs.missing = ref(2)
void refs.missing.value
</script>

<template>
  <p />
</template>
