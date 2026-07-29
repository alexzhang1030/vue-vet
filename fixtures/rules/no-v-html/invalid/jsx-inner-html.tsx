import { defineComponent } from 'vue'

const raw = '<b>x</b>'

export default defineComponent({
  setup() {
    return () => <div innerHTML={raw} />
  },
})
