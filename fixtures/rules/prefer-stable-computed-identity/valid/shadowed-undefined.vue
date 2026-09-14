<script setup lang="ts">
import { computed, ref, watch } from 'vue'

function run(undefined: number) {
  const items = ref([undefined])
  const copy = computed(() => items.value.map((n) => n))
  watch(copy, (value) => {
    void value
  }, { flush: 'sync' })
  items.value = [void 0]
}

run(1)
</script>

<template>
  <p />
</template>
