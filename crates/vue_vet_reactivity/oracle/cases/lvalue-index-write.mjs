/**
 * `target[key.value] = source.value` tracks key and source, not a get of target.
 */
export const id = "lvalue-index-write";

export const source = `import { reactive, ref, watchEffect } from 'vue'
const target = reactive({ field: 0 })
const key = ref('field')
const source = ref(1)
watchEffect(() => {
  target[key.value] = source.value
})
`;

export async function run({ reactive, ref, watchEffect, onTrack }) {
  const target = reactive("target", { field: 0 });
  const key = ref("key", "field");
  const source = ref("source", 1);
  watchEffect(() => {
    target[key.value] = source.value;
  }, { onTrack });
}
