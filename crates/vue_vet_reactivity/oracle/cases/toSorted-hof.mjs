/**
 * Array#toSorted comparator runs synchronously during tracking.
 */
export const id = "toSorted-hof";

export const source = `import { ref, watchEffect } from 'vue'
const list = ref([3, 1, 2])
const key = ref(0)
watchEffect(() => {
  list.value.toSorted((a, b) => a - b + key.value)
})
`;

export async function run({ ref, watchEffect, onTrack }) {
  const list = ref("list", [3, 1, 2]);
  const key = ref("key", 0);
  watchEffect(
    () => {
      list.value.toSorted((a, b) => a - b + key.value);
    },
    { onTrack },
  );
}
