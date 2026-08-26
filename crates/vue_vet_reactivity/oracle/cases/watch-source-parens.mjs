/**
 * watch((ref)) tracks the same .value key as watch(ref).
 */
export const id = "watch-source-parens";

export const source = `import { ref, watch } from 'vue'
const count = ref(1)
watch((count), () => {})
`;

export async function run({ ref, watch, onTrack }) {
  const count = ref("count", 1);
  const stop = watch((count), () => {}, { onTrack });
  await Promise.resolve();
  stop();
}
