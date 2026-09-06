<script setup lang="ts">
import { ref, triggerRef } from 'vue'
let target = 0
;({ target } = { target: ref(1) })
triggerRef(target)
</script>

<template>
  <p />
</template>
