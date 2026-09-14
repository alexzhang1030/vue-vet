<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
const r = ref(0)
function scan() {
  watch(r.value, () => {})
}
r.value = reactive({ n: 1 })
scan()
</script>

<template>
  <p />
</template>
