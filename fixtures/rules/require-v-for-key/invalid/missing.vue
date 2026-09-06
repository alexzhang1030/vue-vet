<script setup>
defineProps(['items'])
</script>
<template>
  <li v-for="item in items">{{ item.name }}</li>
</template>
