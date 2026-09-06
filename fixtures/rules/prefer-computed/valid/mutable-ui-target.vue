<script setup>
import { ref, watchEffect } from 'vue'
const preference = ref(false)
const fallback = ref(true)
const expanded = ref(false)
function useToggle(target) {
  return () => {
    target.value = !target.value
  }
}
const toggle = useToggle(expanded)
watchEffect(() => {
  expanded.value = preference.value === fallback.value
})
</script>
<template><button @click="toggle()">{{ expanded }}</button></template>
