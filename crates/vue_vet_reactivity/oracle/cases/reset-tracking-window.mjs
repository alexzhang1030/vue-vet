/**
 * Reads between pauseTracking/resetTracking are not runtime deps.
 * Reads after resetTracking still track.
 */
export const id = "reset-tracking-window";

export const source = `import { ref, watchEffect, pauseTracking, resetTracking } from 'vue'
const before = ref(1)
const paused = ref(2)
const after = ref(3)
watchEffect(() => {
  void before.value
  pauseTracking()
  void paused.value
  resetTracking()
  void after.value
})
`;

export async function run({ ref, watchEffect, pauseTracking, resetTracking, onTrack }) {
  const before = ref("before", 1);
  const paused = ref("paused", 2);
  const after = ref("after", 3);
  watchEffect(() => {
    void before.value;
    pauseTracking();
    void paused.value;
    resetTracking();
    void after.value;
  }, { onTrack });
}
