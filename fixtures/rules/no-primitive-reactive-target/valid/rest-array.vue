<script setup lang="ts">
import { reactive } from 'vue'
let leftover = 0
;[...leftover] = [reactive({})]
void reactive(leftover)
</script>

<template>
  <p />
</template>
