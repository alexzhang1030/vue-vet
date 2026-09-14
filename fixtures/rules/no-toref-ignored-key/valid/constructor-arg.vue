<script setup lang="ts">
import { ref, toRef } from 'vue'
const created = ref({ count: 1 })
class ClearMarker {
  constructor(value: object) {
    delete (value as { __v_isRef?: boolean }).__v_isRef
  }
}
new ClearMarker(created)
const aliased = toRef(created, 'count')
void aliased
</script>

<template>
  <p />
</template>
