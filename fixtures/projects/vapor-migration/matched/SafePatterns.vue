<script setup>
const go = () => {}
const items = [1]
const attrs = { title: 'ok' }
</script>
<template>
  <div v-once>once</div>
  <div title="Transition">attr</div>
  <p>{{ 'Transition' }}</p>
  <MyTeleport />
  <button @click="go">go</button>
  <li v-for="item in items" :key="item">{{ item }}</li>
  <div v-bind="attrs"></div>
</template>
