/**
 * Synchronous Array#reduceRight callback runs during tracking.
 */
export const id = "sync-reduceRight-hof";

export const source = `import { ref, computed } from 'vue'
const list = ref([1, 2, 3])
const factor = ref(10)
const total = computed(() => list.value.reduceRight((acc, n) => acc + n * factor.value, 0))
void total.value
`;

export async function run({ ref, computed, onTrack }) {
  const list = ref("list", [1, 2, 3]);
  const factor = ref("factor", 10);
  const total = computed(
    () => list.value.reduceRight((acc, n) => acc + n * factor.value, 0),
    { onTrack },
  );
  void total.value;
}
