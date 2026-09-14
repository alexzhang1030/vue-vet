<script setup lang="ts">
import { toRef } from 'vue'
const callable = () => 1
callable.__v_isRef = true
callable.value = 2
const aliased = toRef(callable, 'value')
void aliased
</script>

<template>
  <p />
</template>
