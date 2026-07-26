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
  <input>
</template>
