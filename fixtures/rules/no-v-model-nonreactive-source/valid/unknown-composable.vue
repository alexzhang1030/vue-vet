<script setup lang="ts">
declare function useExternalForm(): { form: { name: string } }
const { form } = useExternalForm()
</script>
<template><FormEditor v-model="form" /></template>
