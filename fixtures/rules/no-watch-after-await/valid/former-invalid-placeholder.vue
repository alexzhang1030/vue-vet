<script setup lang="ts">
import { ref, watch } from 'vue'
const count = ref(0)
await Promise.resolve()
watch(count, () => {})

</script>
