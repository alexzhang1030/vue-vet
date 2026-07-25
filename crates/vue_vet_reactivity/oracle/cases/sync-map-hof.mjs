export const id = "sync-map-hof";

export const source = `import { ref, computed } from 'vue'
const items = ref([1, 2, 3])
const factor = ref(2)
const scaled = computed(() => items.value.map((n) => n * factor.value))
void scaled.value
`;

export async function run({ ref, computed, onTrack }) {
  const items = ref("items", [1, 2, 3]);
  const factor = ref("factor", 2);
  const scaled = computed(() => items.value.map((n) => n * factor.value), {
    onTrack,
  });
  void scaled.value;
}