import { toRef } from 'vue'

export function useField(props: { title: string }) {
  return {
    title: toRef(props, 'title'),
  }
}
