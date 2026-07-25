import { ref } from 'vue'
export function useHidden() {
  const signal = ref(0)
  return { signal }
}
