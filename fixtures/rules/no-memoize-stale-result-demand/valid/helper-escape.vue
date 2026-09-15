<script setup lang="ts">
import { ref } from 'vue'
import { useMemoize } from '@vueuse/core'
function touch(value: unknown) {
  void value
}
const source = ref(1)
const resolve = useMemoize(() => source.value)
resolve()
touch(resolve)
source.value = 'text'
resolve().toUpperCase()
</script>

<template>
  <p />
</template>
