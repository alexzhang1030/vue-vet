/**
 * watchEffect tracks ref.value on the happy path (effect-family baseline).
 */
export const id = "watch-effect-ref";

export const source = `import { ref, watchEffect } from 'vue'
const count = ref(1)
watchEffect(() => {
  void count.value
})
`;

export async function run({ ref, watchEffect, onTrack }) {
  const count = ref("count", 1);
  watchEffect(() => {
    void count.value;
  }, { onTrack });
}
