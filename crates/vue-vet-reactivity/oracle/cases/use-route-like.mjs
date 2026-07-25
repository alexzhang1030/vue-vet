/**
 * useRoute() returns a reactive object; route.path is a member read.
 */
export const id = "use-route-like";

export const source = `import { reactive, computed } from 'vue'
const route = reactive({ path: '/home', name: 'home' })
const title = computed(() => route.path)
void title.value
`;

export async function run({ reactive, computed, onTrack }) {
  const route = reactive("route", { path: "/home", name: "home" });
  const title = computed(() => route.path, { onTrack });
  void title.value;
}
