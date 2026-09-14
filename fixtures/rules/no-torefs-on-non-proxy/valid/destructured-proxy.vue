<script setup lang="ts">
import { reactive, toRefs } from 'vue'
let plain = {}
;({ plain } = { plain: reactive({ count: 1 }) })
toRefs(plain)
</script>

<template>
  <p />
</template>
