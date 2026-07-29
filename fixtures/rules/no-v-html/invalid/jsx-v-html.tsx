import { defineComponent } from 'vue'

const html = '<b>x</b>'

export default defineComponent({
  setup() {
    return () => <div v-html={html} />
  },
})
