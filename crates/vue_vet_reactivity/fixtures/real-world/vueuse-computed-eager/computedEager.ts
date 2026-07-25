import { readonly, shallowRef, watchEffect } from 'vue'

export function useEagerState() {
  const result = shallowRef(0)

  watchEffect(() => {
    result.value = source()
  })

  const exposed = readonly(result)
  return { exposed }
}
