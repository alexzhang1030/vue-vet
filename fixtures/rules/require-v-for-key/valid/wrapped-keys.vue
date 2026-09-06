<script setup>
defineProps(['items'])
</script>
<template>
  <slot v-for="item in items" v-bind="({ key: item.id } as const)" :item="item" />
  <div v-for="item in items" v-bind="({ key: item.id }) satisfies Record<string, unknown>" />
</template>
