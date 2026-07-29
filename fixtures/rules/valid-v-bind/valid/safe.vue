<script setup lang="ts">
import { ref } from 'vue'
const id = ref('main')
</script>
<template>
  <div v-bind:id="id">Bound id</div>
</template>
