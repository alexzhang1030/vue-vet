<script setup lang="ts">
import { reactive, readonly, shallowReactive, shallowReadonly } from 'vue'
void reactive({ n: 0 })
void readonly({ n: 0 })
void shallowReactive({ n: 0 })
void shallowReadonly({ n: 0 })
</script>

<template>
  <p />
</template>
