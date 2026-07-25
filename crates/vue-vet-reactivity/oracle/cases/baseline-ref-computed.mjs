export const id = "baseline-ref-computed";

export const source = `import { ref, computed } from 'vue'
const count = ref(1)
const doubled = computed(() => count.value * 2)
void doubled.value
`;

export async function run({ ref, computed, onTrack }) {
  const count = ref("count", 1);
  const doubled = computed(() => count.value * 2, { onTrack });
  void doubled.value;
}
