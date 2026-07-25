/**
 * Arbitrary .run is not a tracking scope. Runtime records no deps when the
 * callback is not inside computed/watchEffect.
 */
export const id = "runner-run-no-track";

export const source = `import { ref } from 'vue'
const count = ref(0)
const runner = { run(fn) { fn() } }
runner.run(() => count.value)
`;

export async function run({ ref, onTrack }) {
  const count = ref("count", 0);
  const runner = {
    run(fn) {
      fn();
    },
  };
  // onTrack is unused: there is no active reactive effect.
  void onTrack;
  runner.run(() => {
    void count.value;
  });
}
