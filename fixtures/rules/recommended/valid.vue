<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const ready = computed(() => false)
const watchedValue = ref(0)
const templateOnly = ref(1)
watch([ready, watchedValue], () => {
  console.log(watchedValue.value)
})

const componentProps = defineProps<{ title: string }>()
const emit = defineEmits<{ save: [value: string] }>()
const slots = defineSlots<{ default(): unknown }>()
defineExpose({ emit })
defineOptions({ name: 'RecommendedFixture' })
const localTitle = componentProps.title
void slots
</script>

<template>
  <template v-for="item in items" :key="item.id">
    <div v-if="item.visible">{{ item.label }}</div>
  </template>
  <div v-show="visible" />
  <div>{{ localTitle }} · {{ templateOnly }}</div>
  <component :is="currentComponent" />
  <img alt="">
  <iframe title="Preview" />
  <a href="/docs">Documentation</a>
  <button type="button">Save</button>
  <button type="button" @click="activate">Activate</button>
  <input>
  <MyButton @click="activate" />
  <div tabindex="0">Focusable in document order</div>
  <div aria-hidden="true">Decorative duplicate</div>
  <div role="region" aria-label="Status">Ready</div>
  <button role="switch" aria-checked="false">Notifications</button>
  <template #default="{ value }"><span>{{ value }}</span></template>
</template>
