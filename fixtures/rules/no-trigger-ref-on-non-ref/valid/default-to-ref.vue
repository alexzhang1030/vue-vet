<script setup lang="ts">
import { ref, triggerRef } from 'vue'
let boxed = 0
;({ boxed = ref(1) } = {})
triggerRef(boxed)
</script>

<template>
  <p />
</template>
