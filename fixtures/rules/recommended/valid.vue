<script setup lang="ts">
const props = defineProps<{ title: string }>()
const localTitle = props.title
</script>

<template>
  <template v-for="item in items" :key="item.id">
    <div v-if="item.visible">{{ item.label }}</div>
  </template>
  <div v-show="visible" />
  <div>{{ localTitle }}</div>
  <component :is="currentComponent" />
  <img alt="">
  <iframe title="Preview" />
  <a href="/docs">Documentation</a>
  <button type="button">Save</button>
  <button type="button" @click="activate">Activate</button>
  <input>
  <MyButton @click="activate" />
</template>
