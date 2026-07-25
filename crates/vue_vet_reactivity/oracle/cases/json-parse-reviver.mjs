/**
 * JSON.parse reviver runs synchronously during tracking.
 */
export const id = "json-parse-reviver";

export const source = `import { ref, computed } from 'vue'
const raw = ref('{"a":1}')
const flag = ref(true)
const d = computed(() => JSON.parse(raw.value, (k, v) => flag.value ? v : v))
void d.value
`;

export async function run({ ref, computed, onTrack }) {
  const raw = ref("raw", '{"a":1}');
  const flag = ref("flag", true);
  const d = computed(
    () => JSON.parse(raw.value, (k, v) => (flag.value ? v : v)),
    { onTrack },
  );
  void d.value;
}
