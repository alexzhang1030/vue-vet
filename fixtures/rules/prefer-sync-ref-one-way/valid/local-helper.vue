<script setup lang="ts">
import { ref } from 'vue'

function syncRef(left: { value: number }, right: { value: number }) {
  right.value = left.value
}

const source = ref(1)
const display = ref(0)
syncRef(source, display)
source.value = 5
void display.value
</script>

<template>
  <p />
</template>
