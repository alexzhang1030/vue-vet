<script setup>
import { createForm } from 'generic-form-library'
const source = createForm()
const form = source.form
</script>
<template><FormControl v-model="form" /></template>
