<script setup lang="ts">
import { effectScope } from 'vue'
function run() {
  const scope = effectScope()
  scope.stop()
  const result = scope.run(() => ({ count: 1 }))
  function inner() {
    void result.count
  }
}
run()
</script>

<template>
  <p />
</template>
