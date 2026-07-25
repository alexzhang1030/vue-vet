/**
 * watch([a, b]) tracks each ref's .value key during source collection.
 */
export const id = "watch-source-array";

export const source = `import { ref, watch } from 'vue'
const a = ref(1)
const b = ref(2)
watch([a, b], () => {})
`;

export async function run({ ref, watch, onTrack }) {
  const a = ref("a", 1);
  const b = ref("b", 2);
  const stop = watch([a, b], () => {}, { onTrack });
  // Flush the initial source collection.
  await Promise.resolve();
  stop();
}
