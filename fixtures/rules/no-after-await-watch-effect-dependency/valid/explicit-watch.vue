<script setup lang="ts">
import { ref, watch } from 'vue'

const result = ref(0)

watch(
  result,
  async () => {
    await Promise.resolve()
    console.log(result.value)
  },
  { immediate: true },
)
</script>
