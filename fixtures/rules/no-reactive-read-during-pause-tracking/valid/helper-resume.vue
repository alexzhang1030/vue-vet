<script setup lang="ts">
import { enableTracking, pauseTracking, ref, watchEffect } from 'vue'

const value = ref(0)
function load() {
  pauseTracking()
  enableTracking()
  return value.value
}
watchEffect(() => {
  void load()
})
</script>
