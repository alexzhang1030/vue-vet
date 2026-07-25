import { ref, watchEffect } from 'vue'
import { useRouterState } from './router'

const { currentRoute: payload } = useRouterState()
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
