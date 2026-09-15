<!-- 中文前缀 -->
<script setup lang="ts">
import { nextTick, ref, watchEffect } from 'vue'

const value = ref(0)
watchEffect(() => {
  nextTick(() => {
    console.log(value.value)
  })
})
</script>
