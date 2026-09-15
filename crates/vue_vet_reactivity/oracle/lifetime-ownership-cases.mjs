import { computed, customRef, effectScope, getCurrentScope, nextTick, ref, watch, watchEffect } from 'vue';

export async function positiveNested() {
  const owner = effectScope();
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  owner.run(() => {
    watch(outer, () => {
      watch(inner, () => { hits++; }, { flush: 'sync' });
    }, { flush: 'sync' });
  });
  outer.value++;
  outer.value++;
  owner.stop();
  inner.value++;
  return { residualHits: hits };
}

export async function positiveDetached() {
  const owner = effectScope();
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  owner.run(() => {
    watch(outer, () => {
      const scope = effectScope(true);
      scope.run(() => {
        watch(inner, () => { hits++; }, { flush: 'sync' });
      });
    }, { flush: 'sync' });
  });
  outer.value++;
  outer.value++;
  owner.stop();
  inner.value++;
  return { residualHits: hits };
}

export async function dependencyFreeOuter() {
  const owner = effectScope();
  const inner = ref(0);
  let hits = 0;
  let outerRuns = 0;
  owner.run(() => {
    watchEffect(() => {
      outerRuns++;
      watch(inner, () => { hits++; }, { flush: 'sync' });
    });
  });
  inner.value++;
  inner.value++;
  await nextTick();
  owner.stop();
  hits = 0;
  inner.value++;
  return { outerRuns, residualHits: hits };
}

export async function constantOuter() {
  const owner = effectScope();
  const inner = ref(0);
  let hits = 0;
  let outerRuns = 0;
  owner.run(() => {
    watch(() => 1, () => {
      outerRuns++;
      watch(inner, () => { hits++; }, { flush: 'sync' });
    }, { immediate: true, flush: 'sync' });
  });
  inner.value++;
  inner.value++;
  owner.stop();
  hits = 0;
  inner.value++;
  return { outerRuns, residualHits: hits };
}

export async function stoppedOuter() {
  const owner = effectScope();
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  owner.run(() => {
    const stop = watch(outer, () => {
      watch(inner, () => { hits++; }, { flush: 'sync' });
    }, { immediate: true, flush: 'sync' });
    stop();
  });
  outer.value++;
  outer.value++;
  owner.stop();
  inner.value++;
  return { residualHits: hits };
}

export async function exhaustedInnerOnce() {
  const owner = effectScope();
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  owner.run(() => {
    watch(outer, () => {
      watch(inner, () => { hits++; }, { immediate: true, once: true, flush: 'sync' });
    }, { flush: 'sync' });
  });
  outer.value++;
  outer.value++;
  owner.stop();
  const creationHits = hits;
  inner.value++;
  return { creationHits, residualHits: hits - creationHits };
}

export async function getterReturnsRefIdentity() {
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  const stop = watch(outer, () => {
    watch(() => inner, () => { hits++; }, { flush: 'sync' });
  }, { flush: 'sync' });
  outer.value++;
  outer.value++;
  stop();
  inner.value++;
  return { residualHits: hits };
}

export async function effectReadsIdentity() {
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  const stop = watch(outer, () => {
    watchEffect(() => { void inner; hits++; }, { flush: 'sync' });
  }, { flush: 'sync' });
  outer.value++;
  outer.value++;
  stop();
  const creationHits = hits;
  inner.value++;
  return { creationHits, residualHits: hits - creationHits };
}

export async function dormantComputedRetention() {
  const outer = ref(0);
  const inner = computed(() => 1);
  let hits = 0;
  const stop = watch(outer, () => {
    watch(inner, () => { hits++; }, { flush: 'sync' });
  }, { flush: 'sync' });
  outer.value++;
  outer.value++;
  stop();
  return { residualHits: hits, retainedSubscriptions: inner.dep.sc };
}

export async function scopeOnOwnership() {
  const owner = effectScope();
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  owner.run(() => {
    watch(outer, () => {
      owner.on();
      watch(inner, () => { hits++; }, { flush: 'sync' });
      owner.off();
    }, { flush: 'sync' });
  });
  outer.value++;
  outer.value++;
  owner.stop();
  inner.value++;
  return { residualHits: hits };
}

export async function detachedExhaustedInner() {
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  const stop = watch(outer, () => {
    const scope = effectScope(true);
    scope.run(() => {
      watch(inner, () => { hits++; }, { immediate: true, once: true, flush: 'sync' });
    });
  }, { flush: 'sync' });
  outer.value++;
  outer.value++;
  stop();
  const creationHits = hits;
  inner.value++;
  return { creationHits, residualHits: hits - creationHits };
}

export async function detachedUnreachableRun() {
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  const stop = watch(outer, () => {
    const scope = effectScope(true);
    if (false) scope.run(() => {
      watch(inner, () => { hits++; }, { flush: 'sync' });
    });
  }, { flush: 'sync' });
  outer.value++;
  outer.value++;
  stop();
  inner.value++;
  return { residualHits: hits };
}

export async function detachedDynamicRun() {
  const outer = ref(0);
  const inner = ref(0);
  const method = 'run';
  let hits = 0;
  const stop = watch(outer, () => {
    const scope = effectScope(true);
    scope[method] = () => undefined;
    scope.run(() => {
      watch(inner, () => { hits++; }, { flush: 'sync' });
    });
  }, { flush: 'sync' });
  outer.value++;
  outer.value++;
  stop();
  inner.value++;
  return { residualHits: hits };
}

export async function detachedConstructorOwner() {
  const outer = ref(0);
  const inner = ref(0);
  const resources = [];
  class Resource {
    constructor(scope) { this.scope = scope; resources.push(this); }
    stop() { this.scope.stop(); }
  }
  let hits = 0;
  const stop = watch(outer, () => {
    const scope = effectScope(true);
    scope.run(() => {
      watch(inner, () => { hits++; }, { flush: 'sync' });
    });
    new Resource(scope);
  }, { flush: 'sync' });
  outer.value++;
  outer.value++;
  stop();
  resources.forEach(resource => resource.stop());
  inner.value++;
  return { ownedResources: resources.length, residualHits: hits };
}

export async function detachedTrackedScopeOwner() {
  const owner = effectScope();
  const outer = ref(0);
  const inner = ref(0);
  const resources = [];
  let hits = 0;
  owner.run(() => {
    watch(outer, () => {
      const scope = effectScope(true);
      scope.run(() => {
        watch(inner, () => { hits++; }, { flush: 'sync' });
      });
      resources.push(scope);
    }, { flush: 'sync' });
  });
  outer.value++;
  outer.value++;
  owner.stop();
  resources.forEach(scope => scope.stop());
  inner.value++;
  return { ownedResources: resources.length, residualHits: hits };
}

export async function positiveReturnedHandle() {
  const owner = effectScope();
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  owner.run(() => {
    watch(outer, () => {
      return watch(inner, () => { hits++; }, { flush: 'sync' });
    }, { flush: 'sync' });
  });
  outer.value++;
  outer.value++;
  owner.stop();
  inner.value++;
  return { residualHits: hits };
}

export async function positiveSharedCallbackRun() {
  const owner = effectScope();
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  const create = () => {
    watch(inner, () => { hits++; }, { flush: 'sync' });
  };
  owner.run(create);
  owner.run(() => { watch(outer, create, { flush: 'sync' }); });
  outer.value++;
  outer.value++;
  owner.stop();
  inner.value++;
  return { residualHits: hits };
}

export async function unreachableInner() {
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  const stop = watch(outer, () => {
    return;
    watch(inner, () => { hits++; }, { flush: 'sync' });
  }, { flush: 'sync' });
  outer.value++;
  outer.value++;
  stop();
  inner.value++;
  return { residualHits: hits };
}

export async function stableOuterGetter() {
  const outer = ref(0);
  const inner = ref(0);
  const owner = effectScope();
  let hits = 0;
  let outerRuns = 0;
  owner.run(() => {
    watch(() => { void outer.value; return 0; }, () => {
      outerRuns++;
      watch(inner, () => { hits++; }, { flush: 'sync' });
    }, { immediate: true, flush: 'sync' });
  });
  outer.value++;
  outer.value++;
  owner.stop();
  inner.value++;
  return { outerRuns, residualHits: hits };
}

export async function dormantComputedOuter() {
  const outer = computed(() => 0);
  const inner = ref(0);
  const owner = effectScope();
  let hits = 0;
  let outerRuns = 0;
  owner.run(() => {
    watch(outer, () => {
      outerRuns++;
      watch(inner, () => { hits++; }, { flush: 'sync' });
    }, { immediate: true, flush: 'sync' });
  });
  owner.stop();
  inner.value++;
  return { outerRuns, residualHits: hits };
}

export async function deadGetterRead() {
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  const stop = watch(outer, () => {
    watch(() => { return 0; inner.value; }, () => { hits++; }, { flush: 'sync' });
  }, { flush: 'sync' });
  outer.value++;
  outer.value++;
  stop();
  inner.value++;
  return { residualHits: hits, retainedSubscriptions: inner.dep.sc };
}

export async function deleteEffectProperty() {
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  const stop = watch(outer, () => {
    watchEffect(() => { delete inner.value; hits++; }, { flush: 'sync' });
  }, { flush: 'sync' });
  outer.value++;
  outer.value++;
  stop();
  const creationHits = hits;
  inner.value++;
  return { creationHits, residualHits: hits - creationHits, retainedSubscriptions: inner.dep.sc };
}

export async function currentScopeOwner() {
  const outer = ref(0);
  const inner = ref(0);
  const scopes = [];
  let hits = 0;
  const stop = watch(outer, () => {
    const scope = effectScope(true);
    scope.run(() => {
      scopes.push(getCurrentScope());
      watch(inner, () => { hits++; }, { flush: 'sync' });
    });
  }, { flush: 'sync' });
  outer.value++;
  outer.value++;
  stop();
  scopes.forEach(scope => scope.stop());
  inner.value++;
  return { ownedScopes: scopes.length, residualHits: hits };
}

export async function inheritedOptionsRemainRepeatable() {
  const outer = ref(0);
  const inner = ref(0);
  const owner = effectScope();
  let hits = 0;
  let outerRuns = 0;
  owner.run(() => {
    watch(outer, () => {
      outerRuns++;
      watch(inner, () => { hits++; }, { flush: 'sync' });
    }, { __proto__: { once: true, immediate: true }, flush: 'sync' });
  });
  outer.value++;
  outer.value++;
  owner.stop();
  inner.value++;
  return { outerRuns, residualHits: hits };
}

export async function stoppedOuterAfterPureRead() {
  const outer = ref(0);
  const inner = ref(0);
  const owner = effectScope();
  let hits = 0;
  owner.run(() => {
    const stop = watch(outer, () => {
      watch(inner, () => { hits++; }, { flush: 'sync' });
    }, { immediate: true, flush: 'sync' });
    const snapshot = inner.value;
    stop();
    void snapshot;
  });
  outer.value++;
  outer.value++;
  owner.stop();
  inner.value++;
  return { residualHits: hits };
}

export async function returnedTrackedSource() {
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  const stop = watch(outer, () => {
    watchEffect(() => inner.value, { flush: 'sync' });
  }, { flush: 'sync' });
  outer.value++;
  outer.value++;
  stop();
  inner.value++;
  return { residualHits: hits, retainedSubscriptions: inner.dep.sc };
}

export async function assignmentRhsTrackedSource() {
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  let snapshot = 0;
  const stop = watch(outer, () => {
    watchEffect(() => { snapshot = inner.value; hits++; }, { flush: 'sync' });
  }, { flush: 'sync' });
  outer.value++;
  outer.value++;
  stop();
  const creationHits = hits;
  inner.value++;
  return { snapshot, residualHits: hits - creationHits, retainedSubscriptions: inner.dep.sc };
}

export async function effectFamilyOnceIgnored() {
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  const stop = watch(outer, () => {
    watchEffect(() => { void inner.value; hits++; }, { once: true, flush: 'sync' });
  }, { flush: 'sync' });
  outer.value++;
  outer.value++;
  stop();
  const creationHits = hits;
  inner.value++;
  return { residualHits: hits - creationHits, retainedSubscriptions: inner.dep.sc };
}

export async function getterBeforeStop() {
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  const trigger = {
    get value() { outer.value++; outer.value++; return 0; },
  };
  const stop = watch(outer, () => {
    watch(inner, () => { hits++; }, { flush: 'sync' });
  }, { flush: 'sync' });
  const snapshot = trigger.value;
  stop();
  inner.value++;
  return { snapshot, residualHits: hits, retainedSubscriptions: inner.dep.sc };
}

export async function assignmentDefaultRead() {
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  let snapshot = 0;
  const stop = watch(outer, () => {
    watchEffect(() => {
      ({ value: snapshot = inner.value } = {});
      hits++;
    }, { flush: 'sync' });
  }, { flush: 'sync' });
  outer.value++;
  outer.value++;
  stop();
  const creationHits = hits;
  inner.value++;
  return { snapshot, residualHits: hits - creationHits, retainedSubscriptions: inner.dep.sc };
}

export async function assignmentComputedKeyRead() {
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  let snapshot;
  const stop = watch(outer, () => {
    watchEffect(() => {
      ({ [inner.value]: snapshot } = {});
      hits++;
    }, { flush: 'sync' });
  }, { flush: 'sync' });
  outer.value++;
  outer.value++;
  stop();
  const creationHits = hits;
  inner.value++;
  return { residualHits: hits - creationHits, retainedSubscriptions: inner.dep.sc };
}

export async function safeComputedGetterOuter() {
  const source = computed(() => 0);
  const inner = ref(0);
  const owner = effectScope();
  let hits = 0;
  let outerRuns = 0;
  owner.run(() => {
    watch(() => source.value, () => {
      outerRuns++;
      watch(inner, () => { hits++; }, { flush: 'sync' });
    }, { immediate: true, flush: 'sync' });
  });
  owner.stop();
  inner.value++;
  return { outerRuns, residualHits: hits, retainedSubscriptions: inner.dep.sc };
}

export async function objectDefaultSkipped() {
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  let snapshot;
  const stop = watch(outer, () => {
    watchEffect(() => {
      ({ value: snapshot = inner.value } = { value: 42 });
      hits++;
    }, { flush: 'sync' });
  }, { flush: 'sync' });
  outer.value++;
  outer.value++;
  stop();
  const creationHits = hits;
  inner.value++;
  return { snapshot, residualHits: hits - creationHits, retainedSubscriptions: inner.dep.sc };
}

export async function arrayDefaultSkipped() {
  const outer = ref(0);
  const inner = ref(0);
  let hits = 0;
  let snapshot;
  const stop = watch(outer, () => {
    watchEffect(() => {
      [snapshot = inner.value] = [42];
      hits++;
    }, { flush: 'sync' });
  }, { flush: 'sync' });
  outer.value++;
  outer.value++;
  stop();
  const creationHits = hits;
  inner.value++;
  return { snapshot, residualHits: hits - creationHits, retainedSubscriptions: inner.dep.sc };
}

export async function asyncGetterAfterAwait() {
  const source = ref(0);
  const inner = ref(0);
  const owner = effectScope();
  let hits = 0;
  let outerRuns = 0;
  owner.run(() => {
    watch(async () => { await Promise.resolve(); return source.value; }, () => {
      outerRuns++;
      watch(inner, () => { hits++; }, { flush: 'sync' });
    }, { immediate: true, flush: 'sync' });
  });
  await Promise.resolve();
  source.value++;
  source.value++;
  owner.stop();
  inner.value++;
  return { outerRuns, residualHits: hits, retainedSubscriptions: inner.dep.sc };
}

export async function wrappedCustomRefGetter() {
  const outer = ref(0);
  const inner = ref(0);
  const trigger = ref(customRef(() => ({
    get() { outer.value++; outer.value++; return 0; },
    set() {},
  })));
  let hits = 0;
  const stop = watch(outer, () => {
    watch(inner, () => { hits++; }, { flush: 'sync' });
  }, { flush: 'sync' });
  const snapshot = trigger.value;
  stop();
  inner.value++;
  return { snapshot, residualHits: hits, retainedSubscriptions: inner.dep.sc };
}

export async function conditionalCurrentScope() {
  const outer = ref(0);
  const inner = ref(0);
  const retained = [];
  let hits = 0;
  const stop = watch(outer, () => {
    const owner = effectScope(true);
    owner.run(() => {
      if (true) retained.push(getCurrentScope());
      watch(inner, () => { hits++; }, { flush: 'sync' });
    });
  }, { flush: 'sync' });
  outer.value++;
  outer.value++;
  stop();
  for (const scope of retained) scope.stop();
  inner.value++;
  return { residualHits: hits, retainedSubscriptions: inner.dep.sc };
}

export async function cyclicComputedTerminates() {
  const inner = ref(0);
  const left = computed(() => right.value);
  const right = computed(() => left.value);
  const stop = watch(left, () => {
    watch(inner, () => {}, { flush: 'sync' });
  }, { flush: 'sync' });
  return {
    leftUndefined: left.value === undefined,
    rightUndefined: right.value === undefined,
    residualHits: 0,
    retainedSubscriptions: 0,
    stop,
  };
}

export async function lateCurrentScopeKeepsOrphanOwner() {
  const owner = effectScope();
  const inner = ref(0);
  let hits = 0;
  let current;
  await owner.run(async () => {
    await Promise.resolve();
    current = getCurrentScope();
    watchEffect(() => { void inner.value; hits++; }, { flush: 'sync' });
  });
  owner.stop();
  const creationHits = hits;
  inner.value++;
  return { currentScopeMissing: current === undefined, residualHits: hits - creationHits, retainedSubscriptions: inner.dep.sc };
}
