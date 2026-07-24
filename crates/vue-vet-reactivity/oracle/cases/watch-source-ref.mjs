/**
 * watch(ref) tracks the ref's .value key (not a property-less binding).
 */
export const id = "watch-source-ref";

export const source = `import { ref, watch } from 'vue'
const count = ref(1)
watch(count, () => {})
`;

export async function run({ ref, watch, onTrack }) {
  const count = ref("count", 1);
  const stop = watch(count, () => {}, { onTrack });
  await Promise.resolve();
  stop();
}
