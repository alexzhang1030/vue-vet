/**
 * toValue(() => count.value) invokes the getter synchronously during tracking.
 */
export const id = "to-value-getter";

export const source = `import { ref, computed, toValue } from 'vue'
const count = ref(1)
const doubled = computed(() => toValue(() => count.value) * 2)
void doubled.value
`;

export async function run({ ref, computed, toValue, onTrack }) {
  const count = ref("count", 1);
  // Prefer runtime toValue when available; fall back to calling the getter.
  const tv =
    typeof toValue === "function"
      ? toValue
      : (source) => (typeof source === "function" ? source() : source.value);
  const doubled = computed(() => tv(() => count.value) * 2, { onTrack });
  void doubled.value;
}
