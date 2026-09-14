<script setup lang="ts">
import { reactive } from 'vue'
let data = 0
;[data] = [reactive({})]
void reactive(data)
</script>

<template>
  <p />
</template>
