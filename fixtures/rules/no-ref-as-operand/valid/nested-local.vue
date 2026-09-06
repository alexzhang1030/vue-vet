<script setup lang="ts">
import { ref } from 'vue'

const el = ref<HTMLElement>()

function onclick(evt: { composedPath: () => { tagName?: string }[] }) {
  const path = evt.composedPath()
  const el = path.find(node => node.tagName === 'A')
  if (!el)
    return
}
void onclick
</script>

<template>
  <div ref="el" />
</template>
