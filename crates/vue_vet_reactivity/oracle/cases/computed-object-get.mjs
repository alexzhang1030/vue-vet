/**
 * computed({ get, set }) tracks reads inside the getter only.
 */
export const id = "computed-object-get";

export const source = `import { ref, computed } from 'vue'
const count = ref(1)
const doubled = computed({
  get() { return count.value * 2 },
  set(v) { count.value = v },
})
void doubled.value
`;

export async function run({ ref, computed, onTrack }) {
  const count = ref("count", 1);
  const doubled = computed(
    {
      get() {
        return count.value * 2;
      },
      set(v) {
        count.value = v;
      },
    },
    { onTrack },
  );
  void doubled.value;
}
