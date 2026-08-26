/**
 * computed(() => cond ? load() : 0) tracks cond and the helper body when cond is true.
 */
export const id = "computed-helper-ternary";

export const source = `import { ref, computed } from 'vue'
const cond = ref(true)
const count = ref(1)
function load() { return count.value }
const label = computed(() => (cond.value ? load() : 0))
void label.value
`;

export async function run({ ref, computed, onTrack }) {
  const cond = ref("cond", true);
  const count = ref("count", 1);
  function load() {
    return count.value;
  }
  const label = computed(() => (cond.value ? load() : 0), { onTrack });
  void label.value;
}
