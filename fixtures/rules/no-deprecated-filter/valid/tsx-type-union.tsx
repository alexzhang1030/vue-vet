import { defineComponent } from 'vue'

enum PresetAppName {
  Monitor = 'Monitor',
  Dashboard = 'Dashboard',
}

export default defineComponent({
  props: {
    name: { type: String, required: true },
  },
  setup(props) {
    return () => (
      <div type={props.name as PresetAppName.Monitor | PresetAppName.Dashboard} />
    )
  },
})
