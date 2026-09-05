<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const enabled = ref(false)
const result = ref(0)
watchEffect(() => {
  if (!enabled.value) return
  console.log(result.value)
})
</script>
