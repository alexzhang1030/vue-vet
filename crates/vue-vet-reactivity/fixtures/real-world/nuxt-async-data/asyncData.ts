import { computed, shallowRef } from 'vue'

export function useAsyncDataState() {
  const data = shallowRef(0)
  const status = shallowRef('idle')
  const pending = computed(() => status.value === 'pending')

  return { data, pending, status }
}
