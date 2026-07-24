export const id = "watch-effect-await";

export const source = `import { ref, watchEffect } from 'vue'
const title = ref('x')
watchEffect(async () => {
  await Promise.resolve()
  console.log(title.value)
})
`;

export async function run({ ref, watchEffect, onTrack }) {
  const title = ref("title", "x");
  let stop;
  await new Promise((resolve) => {
    stop = watchEffect(
      async () => {
        await Promise.resolve();
        void title.value;
        resolve();
      },
      { onTrack },
    );
  });
  stop?.();
}
