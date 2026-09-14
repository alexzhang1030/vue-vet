<script setup lang="ts">
import { reactive } from 'vue'
;({ ...structuredClone } = { structuredClone: ((value: unknown) => value) as typeof structuredClone })
structuredClone(reactive({ count: 1 }))
</script>

<template>
  <p />
</template>
