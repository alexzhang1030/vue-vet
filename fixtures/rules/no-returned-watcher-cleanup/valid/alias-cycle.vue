<script setup lang="ts">
import { watchEffect } from 'vue'
const a = b
const b = a
watchEffect(a)
</script>
<template>ok</template>
