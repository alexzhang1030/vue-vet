import { watchEffect } from 'vue'
import { useSignal } from './producer'
function local(useSignal: () => { signal: { value: number } }) {
  const { signal: payload } = useSignal()
  watchEffect(() => payload.value)
}
