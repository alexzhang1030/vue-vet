<script setup lang="ts">
import { ref, watch } from 'vue'

const enabled = ref(false)
const result = ref(0)

watch([enabled, result], () => {
  console.log(result.value)
})
</script>
