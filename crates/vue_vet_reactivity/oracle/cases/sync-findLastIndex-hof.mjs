/**
 * Synchronous Array#findLastIndex callback runs during tracking.
 */
export const id = "sync-findLastIndex-hof";

export const source = `import { ref, computed } from 'vue'
const list = ref([1, 2, 3])
const target = ref(2)
const index = computed(() => list.value.findLastIndex((n) => n === target.value))
void index.value
`;

export async function run({ ref, computed, onTrack }) {
  const list = ref("list", [1, 2, 3]);
  const target = ref("target", 2);
  const index = computed(
    () => list.value.findLastIndex((n) => n === target.value),
    { onTrack },
  );
  void index.value;
}
