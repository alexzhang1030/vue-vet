<script setup lang="ts">
import { enableTracking, pauseTracking, ref, watchEffect } from 'vue'

const value = ref(0)
watchEffect(() => {
  pauseTracking()
  console.log(value.value)
  enableTracking()
})
</script>
