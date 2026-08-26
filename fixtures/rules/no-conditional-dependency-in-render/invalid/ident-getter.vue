<script lang="ts">
import { ref } from 'vue'
const enabled = ref(false)
const count = ref(0)
function renderFn() {
  if (!enabled.value) return 0
  return count.value
}
export default { render: renderFn }
</script>
