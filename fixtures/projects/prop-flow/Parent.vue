<script setup lang="ts">
import { reactive, ref } from 'vue'
import Child from './Child.vue'

const label = ref('hello')
const msg = ref('hi')
const bag = reactive({ name: 'n' })
</script>

<template>
  <Child :title="label" v-model="msg" :subtitle="bag.name" :count="msg.value" />
</template>
