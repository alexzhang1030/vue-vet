<script setup lang="ts">
import { ref, toRef } from 'vue'
const 计数 = ref(0)
const aliased = toRef(计数, '键')
void aliased
</script>

<template>
  <p />
</template>
