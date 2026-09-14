<script setup lang="ts">
import { reactive } from 'vue'
function configure(prototype: typeof Array.prototype) {
  const previous = prototype.map
  prototype.__v_skip = true
  prototype.map = () => [7]
  return () => {
    delete prototype.__v_skip
    prototype.map = previous
  }
}
const c0 = Array.prototype
const c1 = c0
const c2 = c1
const c3 = c2
const c4 = c3
const c5 = c4
const c6 = c5
const c7 = c6
const c8 = c7
const c9 = c8
const c10 = c9
const c11 = c10
const c12 = c11
const c13 = c12
const c14 = c13
const c15 = c14
const c16 = c15
const c17 = c16
const c18 = c17
const c19 = c18
const c20 = c19
const c21 = c20
const c22 = c21
const c23 = c22
const c24 = c23
const c25 = c24
const c26 = c25
const c27 = c26
const c28 = c27
const c29 = c28
const c30 = c29
const c31 = c30
const c32 = c31
const restore = configure(c32)
const items = reactive([1])
const { map } = items
map()
restore()
</script>

<template>
  <p />
</template>
