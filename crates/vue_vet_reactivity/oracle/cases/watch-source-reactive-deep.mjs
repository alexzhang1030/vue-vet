/**
 * watch(reactiveObj) deep-tracks at runtime. Static tracer records a deep-root
 * sentinel (`property: "*"`) rather than inventing nested keys.
 */
export const id = "watch-source-reactive-deep";

export const source = `import { reactive, watch } from 'vue'
const state = reactive({ n: 1 })
watch(state, () => {})
`;

export async function run({ reactive, watch, onTrack }) {
  const state = reactive("state", { n: 1 });
  const stop = watch(state, () => {}, { onTrack });
  await Promise.resolve();
  stop();
}
