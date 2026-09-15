<script setup>
import Child from './NonVaporChild.vue'
</script>
<template>
  <Suspense>
    <Child />
  </Suspense>
</template>
