<script setup lang="ts">
import { pauseTracking, ref, watchEffect } from 'vue'

const value = ref(0)
function load() {
  return value.value
}
watchEffect(() => {
  load()
  pauseTracking()
  load()
})
</script>
