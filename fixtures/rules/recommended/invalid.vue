<script setup lang="ts">
import { computed, ref, watchEffect } from 'vue'

const ready = computed(() => false)
const guardedValue = ref(0)
watchEffect(() => {
  if (!ready.value) return
  console.log(guardedValue.value)
})

const componentProps = defineProps<{ title: string }>()
defineProps<{ count: number }>()
defineEmits()
defineEmits()
defineSlots<{ default(): unknown }>()
defineSlots<{ footer(): unknown }>()
defineExpose({ ready: true })
defineExpose({ active: true })
defineOptions({ name: 'FirstName' })
defineOptions({ name: 'SecondName' })
componentProps.title = 'changed'
</script>

<template>
  <div v-for="item in items">{{ item }}</div>
  <div v-for="item in items" v-if="item.visible" :key="item.id" />
  <div v-html>fallback</div>
  <div v-text.trim="label">fallback</div>
  <component />
  <img>
  <iframe />
  <a />
  <button />
  <div @click="activate" />
  <input autofocus>
  <MyButton @click.native="activate" />
  <div tabindex="2" />
  <button aria-hidden="true">Hidden action</button>
  <div role="definitely-invalid" />
  <button role="button">Redundant role</button>
  <template slot-scope="slotProps"><span>{{ slotProps }}</span></template>
  <marquee>Breaking news</marquee>
</template>
