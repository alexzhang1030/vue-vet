/**
 * String#replace replacer function runs synchronously during tracking.
 */
export const id = "string-replace-hof";

export const source = `import { ref, computed } from 'vue'
const text = ref('ab')
const flag = ref(true)
const d = computed(() => text.value.replace(/./g, c => flag.value ? c : ''))
void d.value
`;

export async function run({ ref, computed, onTrack }) {
  const text = ref("text", "ab");
  const flag = ref("flag", true);
  const d = computed(
    () => text.value.replace(/./g, (c) => (flag.value ? c : "")),
    { onTrack },
  );
  void d.value;
}
