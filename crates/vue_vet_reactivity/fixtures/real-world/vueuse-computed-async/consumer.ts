import { ref, watchEffect } from 'vue'
import { useAsyncState } from './computedAsync'

const { current: payload } = useAsyncState()
const ready = ref(false)
const enabled = ref(false)

watchEffect(() => {
  if (!ready.value)
    return

  if (enabled.value) {
    try {
      for (const item of [1]) {
        if (item)
          payload.value
      }
    }
    finally {
      sink()
    }
  }
})
