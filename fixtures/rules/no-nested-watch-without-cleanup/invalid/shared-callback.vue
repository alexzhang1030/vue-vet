<script setup lang="ts">
import { effectScope, ref, watch } from 'vue'
const owner = effectScope()
const outer = ref(0)
const inner = ref(0)
const create = () => {
  watch(inner, () => {}, { flush: 'sync' })
}
owner.run(create)
owner.run(() => {
  watch(outer, create, { flush: 'sync' })
})
</script>
<template>{{ outer }}{{ inner }}</template>
