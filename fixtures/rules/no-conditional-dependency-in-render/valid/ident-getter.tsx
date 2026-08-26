import { defineComponent, ref } from 'vue'

const count = ref(0)

function renderFn() {
  return <p>{count.value}</p>
}

export default defineComponent({
  render: renderFn,
})
