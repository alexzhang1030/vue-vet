<script setup vapor>
import { provide, ref } from 'vue'
const n = ref(0)
const items = ref([1])
provide('key', n)
</script>
<template>
  <input v-model="n" />
  <button @click.stop="n++">inc</button>
  <slot></slot>
  <p v-show="n">shown</p>
  <li v-for="item in items">{{ item }}</li>
  <div v-focus></div>
</template>
