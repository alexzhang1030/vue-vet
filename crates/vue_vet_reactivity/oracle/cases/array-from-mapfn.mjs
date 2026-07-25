/**
 * Array.from(iterable, mapFn) invokes mapFn synchronously during tracking.
 */
export const id = "array-from-mapfn";

export const source = `import { ref, computed } from 'vue'
const list = ref([1, 2])
const factor = ref(2)
const d = computed(() => Array.from(list.value, x => x * factor.value))
void d.value
`;

export async function run({ ref, computed, onTrack }) {
  const list = ref("list", [1, 2]);
  const factor = ref("factor", 2);
  const d = computed(
    () => Array.from(list.value, (x) => x * factor.value),
    { onTrack },
  );
  void d.value;
}
