<script setup lang="ts">
import { computed, ref } from 'vue'

const first = ref('Ada')
const last = ref('Lovelace')
const fullName = computed(() => `${first.value} ${last.value}`)
</script>
