<script setup lang="ts">
import { enableTracking, pauseTracking, ref, watchEffect } from 'vue'

const value = ref(0)
watchEffect(() => {
  const current = value.value
  pauseTracking()
  // non-reactive work while paused
  enableTracking()
  console.log(current)
})
</script>
