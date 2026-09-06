<script setup>
defineProps(['items'])
</script>
<template>
  <li v-for="item in items" :key="item.id">{{ item.name }}</li>
</template>
