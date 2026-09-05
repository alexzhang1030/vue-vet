<script setup lang="ts">
import { ref, watchEffect } from 'vue'

const n = ref(1)
const total = ref(0)

watchEffect(() => {
  total.value += n.value
})
</script>
