<script setup lang="ts">
import { effectScope, ref } from 'vue'
const enabled = ref(false)
const count = ref(0)
const scope = effectScope()
scope.run(() => {
  if (!enabled.value) return
  console.log(count.value)
})
</script>
