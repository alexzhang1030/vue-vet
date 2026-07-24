/**
 * Synchronous Array#forEach callback runs during tracking.
 */
export const id = "sync-forEach-hof";

export const source = `import { ref, computed } from 'vue'
const list = ref([1, 2, 3])
const factor = ref(2)
const scaled = computed(() => {
  const out = []
  list.value.forEach((n) => {
    out.push(n * factor.value)
  })
  return out
})
void scaled.value
`;

export async function run({ ref, computed, onTrack }) {
  const list = ref("list", [1, 2, 3]);
  const factor = ref("factor", 2);
  const scaled = computed(() => {
    const out = [];
    list.value.forEach((n) => {
      out.push(n * factor.value);
    });
    return out;
  }, { onTrack });
  void scaled.value;
}
