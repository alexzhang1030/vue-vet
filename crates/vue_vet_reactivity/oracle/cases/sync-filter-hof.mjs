/**
 * Synchronous Array#filter callback runs during tracking and must track query.
 */
export const id = "sync-filter-hof";

export const source = `import { ref, computed } from 'vue'
const list = ref(['ada', 'linus'])
const query = ref('a')
const filtered = computed(() => list.value.filter((item) => item.includes(query.value)))
void filtered.value
`;

export async function run({ ref, computed, onTrack }) {
  const list = ref("list", ["ada", "linus"]);
  const query = ref("query", "a");
  const filtered = computed(
    () => list.value.filter((item) => item.includes(query.value)),
    { onTrack },
  );
  void filtered.value;
}
