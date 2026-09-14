<script setup lang="ts">
import { toRef } from 'vue'
const marker = { __v_isRef: true, value: 0 }
const aliased = toRef(marker, 'n')
void aliased
</script>

<template>
  <p />
</template>
