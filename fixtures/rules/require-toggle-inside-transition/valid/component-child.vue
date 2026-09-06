<script setup>
import { ref } from 'vue'
import ToggleChild from './ToggleChild.vue'
const visible = ref(true)
</script>
<template>
  <Transition>
    <ToggleChild :visible="visible" />
  </Transition>
</template>
