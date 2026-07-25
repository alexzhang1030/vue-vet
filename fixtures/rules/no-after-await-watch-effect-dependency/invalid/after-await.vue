<script setup lang="ts">
import { ref, watchEffect } from 'vue'

const result = ref(0)

watchEffect(async () => {
  await Promise.resolve()
  console.log(result.value)
})
</script>
