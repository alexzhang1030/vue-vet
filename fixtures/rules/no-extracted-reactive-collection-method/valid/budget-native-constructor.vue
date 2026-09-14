<script setup lang="ts">
import { reactive } from 'vue'
function configure(ctor: ArrayConstructor) {
  const previous = ctor.prototype.map
  ctor.prototype.__v_skip = true
  ctor.prototype.map = () => [7]
  return () => {
    delete ctor.prototype.__v_skip
    ctor.prototype.map = previous
  }
}
const restore = configure(
  true && (true && (true && (true && (true && (true && (true && (true && Array))))))),
)
const items = reactive([1])
const { map } = items
map()
restore()
</script>

<template>
  <p />
</template>
