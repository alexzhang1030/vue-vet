<script setup lang="ts">
import { reactive, toRefs } from 'vue'
void toRefs(reactive({ __proto__: { inherited: 1 }, count: 1 })).inherited.value
</script>

<template>
  <p />
</template>
