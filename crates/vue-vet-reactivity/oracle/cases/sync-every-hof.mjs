/**
 * Synchronous Array#every callback runs during tracking.
 */
export const id = "sync-every-hof";

export const source = `import { ref, computed } from 'vue'
const list = ref([1, 2, 3])
const threshold = ref(0)
const ok = computed(() => list.value.every((n) => n > threshold.value))
void ok.value
`;

export async function run({ ref, computed, onTrack }) {
  const list = ref("list", [1, 2, 3]);
  const threshold = ref("threshold", 0);
  const ok = computed(
    () => list.value.every((n) => n > threshold.value),
    { onTrack },
  );
  void ok.value;
}
