<script setup lang="ts">
import { toRefs } from 'vue'

const props = defineProps<{ title: string }>()
const { title } = toRefs(props)
console.log(title.value)
</script>
