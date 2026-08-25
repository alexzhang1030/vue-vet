/**
 * computed(load) tracks the same getter body as computed(() => load()).
 */
export const id = "computed-fn-ref";

export const source = `import { ref, computed } from 'vue'
const count = ref(1)
function load() { return count.value * 2 }
const doubled = computed(load)
void doubled.value
`;

export async function run({ ref, computed, onTrack }) {
  const count = ref("count", 1);
  function load() {
    return count.value * 2;
  }
  const doubled = computed(load, { onTrack });
  void doubled.value;
}
