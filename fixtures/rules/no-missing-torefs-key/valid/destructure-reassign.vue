<script setup lang="ts">
import { reactive, ref, toRefs } from 'vue'
let { missing } = toRefs(reactive({ count: 1 }))
missing = ref(2)
void missing.value
</script>

<template>
  <p />
</template>
