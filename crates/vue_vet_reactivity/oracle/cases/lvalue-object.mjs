/**
 * Nested assignment `draft.value.params.x = time.value` gets draft.value (and
 * does not treat a plain `out.value = source.value` as a get of out.value).
 */
export const id = "lvalue-object";

export const source = `import { ref, watchEffect } from 'vue'
const draft = ref({ params: { scheduledAt: '' } })
const time = ref('09:00')
watchEffect(() => {
  draft.value.params.scheduledAt = time.value
})
`;

export async function run({ ref, watchEffect, onTrack }) {
  const draft = ref("draft", { params: { scheduledAt: "" } });
  const time = ref("time", "09:00");
  watchEffect(() => {
    draft.value.params.scheduledAt = time.value;
  }, { onTrack });
}
