<script setup lang="ts">
import { reactive, ref, triggerRef } from 'vue'
const r = reactive(ref(0))
triggerRef(r)
</script>

<template>
  <p />
</template>
