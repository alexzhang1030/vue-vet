<script setup lang="ts">
import { reactive } from 'vue'
;({ clone: globalThis.structuredClone } = {
  clone: ((value: unknown) => value) as typeof structuredClone,
})
structuredClone(reactive({ count: 1 }))
</script>

<template>
  <p />
</template>
