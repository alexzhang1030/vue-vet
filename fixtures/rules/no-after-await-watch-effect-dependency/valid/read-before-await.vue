<script setup lang="ts">
import { ref, watchEffect } from 'vue'

const result = ref(0)

watchEffect(async () => {
  const current = result.value
  await Promise.resolve()
  console.log(current)
})
</script>
