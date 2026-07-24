/**
 * Synchronous Array#find callback runs during tracking.
 */
export const id = "sync-find-hof";

export const source = `import { ref, computed } from 'vue'
const list = ref([1, 2, 3])
const target = ref(2)
const hit = computed(() => list.value.find((n) => n === target.value))
void hit.value
`;

export async function run({ ref, computed, onTrack }) {
  const list = ref("list", [1, 2, 3]);
  const target = ref("target", 2);
  const hit = computed(
    () => list.value.find((n) => n === target.value),
    { onTrack },
  );
  void hit.value;
}
