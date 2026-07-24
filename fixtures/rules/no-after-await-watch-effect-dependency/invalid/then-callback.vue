<script setup lang="ts">
import { ref, watchEffect } from 'vue'

const result = ref(0)

watchEffect(() => {
  Promise.resolve().then(() => {
    console.log(result.value)
  })
})
</script>
