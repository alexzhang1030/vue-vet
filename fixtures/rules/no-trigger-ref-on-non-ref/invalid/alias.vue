<script setup lang="ts">
import { reactive, triggerRef as ping } from 'vue'
const obj = reactive({ n: 1 })
ping(obj)
</script>

<template>
  <p />
</template>
