import { watchEffect } from 'vue'
import { signal as payload } from './producer'
watchEffect(() => payload.value)
