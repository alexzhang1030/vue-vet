/**
 * pauseTracking inside a followed helper drops the helper read.
 * enableTracking after the call still tracks later sibling reads.
 */
export const id = "pause-tracking-helper";

export const source = `import { ref, watchEffect, pauseTracking, enableTracking } from 'vue'
const paused = ref(1)
const after = ref(2)
function load() {
  pauseTracking()
  return paused.value
}
watchEffect(() => {
  void load()
  enableTracking()
  void after.value
})
`;

export async function run({ ref, watchEffect, pauseTracking, enableTracking, onTrack }) {
  const paused = ref("paused", 1);
  const after = ref("after", 2);
  function load() {
    pauseTracking();
    return paused.value;
  }
  watchEffect(() => {
    void load();
    enableTracking();
    void after.value;
  }, { onTrack });
}
