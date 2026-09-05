<script lang="ts">
import { ref } from 'vue'
const other = ref(0)
const count = other
</script>
<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const count = ref(0)
const alias = count
watchEffect(() => {
  alias.value = count.value + 1
})
</script>
<template>{{ count }}</template>
