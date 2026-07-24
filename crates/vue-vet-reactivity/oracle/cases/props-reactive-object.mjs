/**
 * Stand-in for defineProps output: a reactive object read as props.count.
 * Static tracer should model defineProps() assignment the same way.
 */
export const id = "props-reactive-object";

export const source = `import { reactive, computed } from 'vue'
const props = reactive({ count: 1 })
const doubled = computed(() => props.count * 2)
void doubled.value
`;

export async function run({ reactive, computed, onTrack }) {
  const props = reactive("props", { count: 1 });
  const doubled = computed(() => props.count * 2, { onTrack });
  void doubled.value;
}
