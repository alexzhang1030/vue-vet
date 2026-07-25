/**
 * Synchronous Array#flatMap callback runs during tracking.
 */
export const id = "sync-flatMap-hof";

export const source = `import { ref, computed } from 'vue'
const list = ref([[1], [2]])
const n = ref(0)
const out = computed(() => list.value.flatMap((xs) => xs.map((x) => x + n.value)))
void out.value
`;

export async function run({ ref, computed, onTrack }) {
  const list = ref("list", [[1], [2]]);
  const n = ref("n", 0);
  const out = computed(
    () => list.value.flatMap((xs) => xs.map((x) => x + n.value)),
    { onTrack },
  );
  void out.value;
}
