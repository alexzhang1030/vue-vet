<script setup>
import { ref, watchEffect } from 'vue'
const source = ref(1)
const extra = ref(0)
const target = ref(0)
watchEffect(() => { target.value = source.value + extra.value })
</script>
<template><InputControl @reset-state="target = 0" /></template>
