import { computed, reactive } from 'vue'
const keyed = reactive(new Map([['selected', 1], ['other', 2]]))
const selected = computed(() => {
  let value
  keyed.forEach((entry, key) => {
    if (key === 'selected') value = entry
  })
  return value
})
void selected
