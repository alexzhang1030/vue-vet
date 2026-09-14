<script setup lang="ts">
import { ref } from 'vue'
import * as Shared from '@vueuse/shared'
const revision = ref(0)
const source = ref(1)
const value = Shared.computedWithControl(revision, () => source.value)
void value.value
source.value = 'text'
value.value.toUpperCase()
</script>

<template>
  <p />
</template>
