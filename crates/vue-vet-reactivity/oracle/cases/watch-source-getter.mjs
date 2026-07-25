/**
 * watch(() => value.value) tracks the getter body like a computed source.
 */
export const id = "watch-source-getter";

export const source = `import { ref, watch } from 'vue'
const value = ref(0)
watch(() => value.value, () => {})
`;

export async function run({ ref, watch, onTrack }) {
  const value = ref("value", 0);
  const stop = watch(() => value.value, () => {}, { onTrack });
  await Promise.resolve();
  stop();
}
