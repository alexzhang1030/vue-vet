import { ref } from 'vue'
export function useSignal() {
  const signal = ref(0)
  return { signal }
}
