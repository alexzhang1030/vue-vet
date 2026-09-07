<script setup lang="ts">
import { customRef, type Ref } from 'vue'
const count = customRef(() => ({
  set() {},
})) as Ref<number>
void count.value
</script>

<template>
  <p />
</template>
