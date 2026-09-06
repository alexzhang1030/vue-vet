<script setup>
defineProps(['items'])
</script>
<template>
  <slot v-for="item in items" v-bind="{ key: item.id }" :item="item" />
</template>
