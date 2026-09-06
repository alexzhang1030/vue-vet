<script setup>
const id = 'panel'
</script>
<template>
  <Transition>
    <div :key="id">Keyed native child</div>
  </Transition>
</template>
