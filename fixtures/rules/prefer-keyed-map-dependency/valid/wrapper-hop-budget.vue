<script setup lang="ts">
import { computed, reactive, shallowReactive } from 'vue'
const inner = new Map([['selected', 7]])
const wrapper0 = reactive(inner)
const wrapper1 = shallowReactive(wrapper0)
const wrapper2 = shallowReactive(wrapper1)
const wrapper3 = shallowReactive(wrapper2)
const wrapper4 = shallowReactive(wrapper3)
const wrapper5 = shallowReactive(wrapper4)
const wrapper6 = shallowReactive(wrapper5)
const wrapper7 = shallowReactive(wrapper6)
const wrapper8 = shallowReactive(wrapper7)
const wrapper9 = shallowReactive(wrapper8)
const wrapper10 = shallowReactive(wrapper9)
wrapper10.get = () => 9
const keyed = reactive(inner)
const selected = computed(() => {
  let value
  keyed.forEach((entry, key) => {
    if (key === 'selected') value = entry
  })
  return value
})
void selected
</script>

<template>
  <p />
</template>
