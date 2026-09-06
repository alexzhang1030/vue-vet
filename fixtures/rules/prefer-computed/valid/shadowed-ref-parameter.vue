<script setup lang="ts">
import { ref, watchEffect, type Ref } from 'vue'

const count = ref(0)
const source = ref(1)

function sync(count: Ref<number>) {
  watchEffect(() => {
    count.value = source.value
  })
}

sync(count)
</script>

<template>{{ count }}</template>
