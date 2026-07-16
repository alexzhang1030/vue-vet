<script setup lang="ts">
const props = defineProps<{ title: string }>()
defineProps<{ count: number }>()
props.title = 'changed'
</script>

<template>
  <div v-for="item in items">{{ item }}</div>
  <div v-for="item in items" v-if="item.visible" :key="item.id" />
  <div v-html>fallback</div>
  <div v-text.trim="label">fallback</div>
  <component />
  <img>
  <iframe />
  <a />
  <button />
  <div @click="activate" />
  <input autofocus>
  <MyButton @click.native="activate" />
</template>
