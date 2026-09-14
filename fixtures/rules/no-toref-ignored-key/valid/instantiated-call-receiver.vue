<script setup lang="ts">
import { ref, toRef } from 'vue'
const tagged = ref({ count: 1 })
tagged.clear = function () {
  delete this.__v_isRef
}
tagged.clear<number>()
const aliased = toRef(tagged, 'count')
void aliased
</script>

<template>
  <p />
</template>
