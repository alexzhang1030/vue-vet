<script setup lang="ts">
import { computed, ref, watch } from 'vue'

const enabled = computed(() => false)
const result = ref(0)

watch([enabled, result], () => {
  console.log(result.value)
})
</script>
