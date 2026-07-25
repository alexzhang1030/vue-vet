/**
 * Synchronous Array#some callback runs during tracking.
 */
export const id = "sync-some-hof";

export const source = `import { ref, computed } from 'vue'
const list = ref([1, 2, 3])
const threshold = ref(2)
const hasBig = computed(() => list.value.some((n) => n > threshold.value))
void hasBig.value
`;

export async function run({ ref, computed, onTrack }) {
  const list = ref("list", [1, 2, 3]);
  const threshold = ref("threshold", 2);
  const hasBig = computed(
    () => list.value.some((n) => n > threshold.value),
    { onTrack },
  );
  void hasBig.value;
}
