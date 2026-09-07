import { defineComponent, ref } from 'vue'

const enabled = ref(false)
const count = ref(0)

function renderFn() {
  if (!enabled.value) {
    return <p>off</p>
  }
  return <p>{count.value}</p>
}

export default defineComponent({
  render: renderFn,
})
