import { computed, toRef } from 'vue'

export function storeToRefs(store) {
  const refs = {}

  for (const key in store) {
    const value = store[key]
    if (value?.effect)
      refs[key] = computed(() => store[key])
    else
      refs[key] = toRef(store, key)
  }

  return refs
}
