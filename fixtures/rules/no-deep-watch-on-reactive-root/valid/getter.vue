<script setup lang="ts">
import { reactive, watch } from 'vue'

const state = reactive({ count: 0 })
watch(
  () => state.count,
  (count) => {
    console.log(count)
  },
)
</script>
