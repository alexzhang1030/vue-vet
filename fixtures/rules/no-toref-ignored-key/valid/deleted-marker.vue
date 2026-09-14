<script setup lang="ts">
import { ref, toRef } from 'vue'
const object = ref(1)
delete object.__v_isRef
const future = toRef(object, 'future')
void future
</script>

<template>
  <p />
</template>
