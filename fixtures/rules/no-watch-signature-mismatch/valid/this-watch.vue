<script lang="ts">
import { defineComponent } from 'vue'
export default defineComponent({
  watch: {},
  created() {
    this.$watch('n', { handler() {} })
  },
})
</script>

<template>
  <p />
</template>
