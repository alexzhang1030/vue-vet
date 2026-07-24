/**
 * reactive object member access tracks the property key.
 */
export const id = "reactive-member";

export const source = `import { reactive, computed } from 'vue'
const state = reactive({ count: 1 })
const doubled = computed(() => state.count * 2)
void doubled.value
`;

export async function run({ reactive, computed, onTrack }) {
  const state = reactive("state", { count: 1 });
  const doubled = computed(() => state.count * 2, { onTrack });
  void doubled.value;
}
