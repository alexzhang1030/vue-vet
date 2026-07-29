<script setup lang="ts">
import { reactive, watch } from 'vue'

const state = reactive({ count: 0 })
watch(state, () => {
  console.log(state.count)
})
</script>
