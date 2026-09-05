import { defineComponent, ref } from 'vue'

const enabled = ref(false)
const count = ref(0)

export default defineComponent(() => {
  return () => {
    if (!enabled.value) {
      return <p>off</p>
    }
    return <p>{count.value}</p>
  }
})
