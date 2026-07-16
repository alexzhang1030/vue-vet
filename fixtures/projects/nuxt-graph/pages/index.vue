<script setup lang="ts">
import AppCard from '@/components/AppCard.vue'

const account = useAccount()
</script>

<template>
  <AppCard />
  <AutoBadge />
  <p>{{ account.name }}</p>
</template>
