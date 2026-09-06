<script setup>
defineProps(['items'])
</script>
<template>
  <div v-for="item in items" v-bind="{ key: item.id, 1: true }" />
  <div v-for="item in items" v-bind="{ key: item.id, [2]: true }" />
</template>
