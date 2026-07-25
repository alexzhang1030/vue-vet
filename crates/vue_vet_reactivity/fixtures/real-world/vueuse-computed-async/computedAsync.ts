import { shallowRef, watchEffect } from 'vue'

export function useAsyncState() {
  const started = shallowRef(false)
  const current = shallowRef(0)

  watchEffect(async () => {
    if (!started.value)
      return

    await Promise.resolve()
    current.value = 1
  })

  return { started, current }
}
