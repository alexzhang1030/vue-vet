<script setup lang="ts">
import { ref, watchEffect } from 'vue'

const first = ref('Ada')
const last = ref('Lovelace')
const fullName = ref('')

function assign() {
  fullName.value = `${first.value} ${last.value}`
}

watchEffect(() => {
  assign()
})
</script>
