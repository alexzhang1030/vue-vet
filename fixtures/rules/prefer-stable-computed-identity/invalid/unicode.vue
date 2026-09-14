<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const 列表 = ref([1, 2])
const 加倍 = computed(() => 列表.value.map((n) => n * 2))
watch(加倍, (value) => {
  void value
})
列表.value = [1, 2]
</script>

<template>
  <p />
</template>
