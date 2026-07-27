<script setup lang="ts">
import { reactive, ref } from 'vue'
import Child from './Child.vue'
import SpreadChild from './SpreadChild.vue'

const label = ref('hello')
const msg = ref('hi')
const bag = reactive({ name: 'n', title: 'spread' })
</script>

<template>
  <Child :title="label" v-model="msg" :subtitle="bag.name" :count="msg.value" />
  <!-- whole-object v-bind stays under-approx quiet (no Prop edges) -->
  <SpreadChild v-bind="bag" />
</template>
