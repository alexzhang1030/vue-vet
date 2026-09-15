<script setup lang="ts">
let timer: ReturnType<typeof setTimeout> | undefined
function search(query: string) {
  clearTimeout(timer)
  timer = setTimeout(() => {
    console.log(query)
  }, 200)
}
void search
</script>

<template>
  <p>中文前缀</p>
  <input aria-label="Search">
</template>
