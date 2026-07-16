<script setup lang="ts">
import { computed, ref, watchEffect } from 'vue'

const enabled = computed(() => false)
const result = ref(0)

watchEffect(() => {
  if (!enabled.value) return
  console.log(result.value)
})
</script>
