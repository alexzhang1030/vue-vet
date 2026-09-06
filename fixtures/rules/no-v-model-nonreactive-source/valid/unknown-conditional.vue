<script setup>
import { createForm } from 'generic-form-library'
const otherForm = {}
const form = Math.random() > 0.5 ? createForm() : otherForm
</script>
<template><FormControl v-model="form" /></template>
