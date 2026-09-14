<script setup lang="ts">
import { ref, toRef } from 'vue'
const pattern = ref({ count: 1 })
;({ flag: pattern.__v_isRef } = { flag: false })
const aliased = toRef(pattern, 'count')
void aliased
</script>

<template>
  <p />
</template>
