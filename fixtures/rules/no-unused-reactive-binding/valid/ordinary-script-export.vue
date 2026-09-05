<script lang="ts">
import { computed, ref } from 'vue'
export const count = ref(1)
export const doubled = computed(() => count.value * 2)
export function useCount() {
  const local = ref(1)
  return { local }
}
export default {
  name: 'Exported',
}
</script>
