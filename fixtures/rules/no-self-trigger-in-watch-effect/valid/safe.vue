<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const src = ref(0)
const dst = ref(0)
watchEffect(() => { dst.value = src.value })
</script>
