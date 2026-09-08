<script setup lang="ts">
import { customRef } from 'vue'
const count = customRef(() => ({
  set() {},
}))
delete count.value
</script>

<template>
  <p />
</template>
