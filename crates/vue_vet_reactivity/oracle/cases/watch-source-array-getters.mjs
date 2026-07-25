/**
 * watch([() => a.value, () => b.value]) tracks each getter body.
 */
export const id = "watch-source-array-getters";

export const source = `import { ref, watch } from 'vue'
const a = ref(1)
const b = ref(2)
watch([() => a.value, () => b.value], () => {})
`;

export async function run({ ref, watch, onTrack }) {
  const a = ref("a", 1);
  const b = ref("b", 2);
  watch([() => a.value, () => b.value], () => {}, { onTrack });
}
