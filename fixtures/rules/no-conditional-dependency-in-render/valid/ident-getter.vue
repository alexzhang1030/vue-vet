<script lang="ts">
import { ref } from 'vue'
const count = ref(0)
function renderFn() {
  return count.value
}
export default { render: renderFn }
</script>
