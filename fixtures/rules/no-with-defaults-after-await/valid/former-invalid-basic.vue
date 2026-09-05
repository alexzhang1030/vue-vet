<script setup lang="ts">
await Promise.resolve()
withDefaults(defineProps<{ n?: number }>(), { n: 0 })

</script>
