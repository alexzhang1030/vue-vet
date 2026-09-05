<script setup lang="ts">
import { ref, watch } from 'vue'
const enabled = ref(false)
const count = ref(0)
watch(
  () => (enabled.value ? count.value : 0),
  () => {},
)
</script>
