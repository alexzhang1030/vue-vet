/**
 * Synchronous Array#reduce callback runs during tracking and must track factor.
 */
export const id = "sync-reduce-hof";

export const source = `import { ref, computed } from 'vue'
const list = ref([1, 2, 3])
const factor = ref(10)
const total = computed(() => list.value.reduce((sum, n) => sum + n * factor.value, 0))
void total.value
`;

export async function run({ ref, computed, onTrack }) {
  const list = ref("list", [1, 2, 3]);
  const factor = ref("factor", 10);
  const total = computed(
    () => list.value.reduce((sum, n) => sum + n * factor.value, 0),
    { onTrack },
  );
  void total.value;
}
