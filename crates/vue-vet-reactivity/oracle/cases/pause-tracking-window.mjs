/**
 * Reads between pauseTracking/enableTracking are not runtime deps.
 * Reads outside that window still track.
 */
export const id = "pause-tracking-window";

export const source = `import { ref, watchEffect, pauseTracking, enableTracking } from 'vue'
const before = ref(1)
const paused = ref(2)
const after = ref(3)
watchEffect(() => {
  void before.value
  pauseTracking()
  void paused.value
  enableTracking()
  void after.value
})
`;

export async function run({ ref, watchEffect, pauseTracking, enableTracking, onTrack }) {
  const before = ref("before", 1);
  const paused = ref("paused", 2);
  const after = ref("after", 3);
  watchEffect(() => {
    void before.value;
    pauseTracking();
    void paused.value;
    enableTracking();
    void after.value;
  }, { onTrack });
}
