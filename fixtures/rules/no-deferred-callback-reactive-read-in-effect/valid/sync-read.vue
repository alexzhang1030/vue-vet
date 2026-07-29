<script setup lang="ts">
import { nextTick, ref, watchEffect } from 'vue'

const value = ref(0)
watchEffect(() => {
  const current = value.value
  nextTick(() => {
    console.log(current)
  })
})
</script>
