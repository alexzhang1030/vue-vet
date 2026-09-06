<script setup>
defineProps(['items'])
</script>
<template>
  <li v-for="item in items" v-bind="{ key: item.id, class: 'row' }">{{ item.name }}</li>
</template>
