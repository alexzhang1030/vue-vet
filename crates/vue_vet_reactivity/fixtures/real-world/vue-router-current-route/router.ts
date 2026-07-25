import { shallowRef } from 'vue'

export function useRouterState() {
  const currentRoute = shallowRef({ matched: [] })
  return { currentRoute }
}
