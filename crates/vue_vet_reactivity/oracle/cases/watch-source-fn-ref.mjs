/**
 * watch(load) treats a local function reference as a source getter.
 */
export const id = "watch-source-fn-ref";

export const source = `import { ref, watch } from 'vue'
const value = ref(0)
function load() { return value.value }
watch(load, () => {})
`;

export async function run({ ref, watch, onTrack }) {
  const value = ref("value", 0);
  function load() {
    return value.value;
  }
  const stop = watch(load, () => {}, { onTrack });
  await Promise.resolve();
  stop();
}
