<script setup lang="ts">
import { ref, onMounted } from 'vue'
import Child from './Child.vue'
const 左 = ref(null)
const 右 = ref(null)
onMounted(() => {
  左.value.model.n = 'text'
  右.value.model.n.toFixed(2)
})
</script>
<template>
  <Child ref="左" />
  <Child ref="右" />
</template>
