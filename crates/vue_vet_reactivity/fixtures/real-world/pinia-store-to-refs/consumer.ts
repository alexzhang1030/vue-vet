import { ref, watchEffect } from 'vue'
import { storeToRefs } from './storeToRefs'

const { count: payload } = storeToRefs({ count: 0 })
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
