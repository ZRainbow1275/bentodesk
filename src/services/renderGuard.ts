import { getSettings } from "../stores/settings";
import type { SafetyProfile } from "../types/settings";
import {
  getRuntimeHealthSnapshot,
  subscribeRuntimeHealth,
  type RuntimeEffectsMode,
  type RuntimePressureLevel,
} from "./runtimeHealth";

interface RenderBudget {
  initialRenderLimit: number;
  renderStep: number;
  iconPreloadBatch: number;
  domSoftLimit: number;
  emergencyRenderLimit: number;
}

type RenderGuardListener = (snapshot: RenderGuardSnapshot) => void;

export interface RenderGuardSnapshot {
  configuredProfile: SafetyProfile;
  hardwareProfile: SafetyProfile;
  effectiveProfile: SafetyProfile;
  hardwareConcurrency: number | null;
  deviceMemoryGb: number | null;
  domNodes: number;
  domSoftLimit: number;
  initialRenderLimit: number;
  renderStep: number;
  iconPreloadBatch: number;
  runtimePressure: RuntimePressureLevel;
  effectsMode: RuntimeEffectsMode;
  runtimeReasons: string[];
  runtimeSampleCount: number;
  blocked: boolean;
  reason: "dom-soft-limit" | "dom-low-headroom" | "runtime-pressure" | null;
}

const PROFILE_ORDER: Record<SafetyProfile, number> = {
  Conservative: 0,
  Balanced: 1,
  Expanded: 2,
};

const ESTIMATED_DOM_NODES_PER_ITEM = 8;
const renderGuardListeners = new Set<RenderGuardListener>();
let renderGuardObserver: MutationObserver | null = null;
let renderGuardTickScheduled = false;
let runtimeHealthCleanup: (() => void) | null = null;

const PROFILE_BUDGETS: Record<SafetyProfile, RenderBudget> = {
  Conservative: {
    initialRenderLimit: 72,
    renderStep: 48,
    iconPreloadBatch: 48,
    domSoftLimit: 1800,
    emergencyRenderLimit: 36,
  },
  Balanced: {
    initialRenderLimit: 160,
    renderStep: 120,
    iconPreloadBatch: 120,
    domSoftLimit: 3200,
    emergencyRenderLimit: 72,
  },
  Expanded: {
    initialRenderLimit: 240,
    renderStep: 160,
    iconPreloadBatch: 160,
    domSoftLimit: 4600,
    emergencyRenderLimit: 96,
  },
};

function getHardwareSignals(): {
  hardwareConcurrency: number | null;
  deviceMemoryGb: number | null;
} {
  if (typeof navigator === "undefined") {
    return {
      hardwareConcurrency: null,
      deviceMemoryGb: null,
    };
  }

  const deviceMemory = (
    navigator as Navigator & { deviceMemory?: number }
  ).deviceMemory;

  return {
    hardwareConcurrency: navigator.hardwareConcurrency ?? null,
    deviceMemoryGb: typeof deviceMemory === "number" ? deviceMemory : null,
  };
}

function detectHardwareProfile(): SafetyProfile {
  const { hardwareConcurrency, deviceMemoryGb } = getHardwareSignals();

  if (
    (hardwareConcurrency !== null && hardwareConcurrency <= 4) ||
    (deviceMemoryGb !== null && deviceMemoryGb <= 4)
  ) {
    return "Conservative";
  }

  if (
    hardwareConcurrency !== null &&
    hardwareConcurrency >= 12 &&
    (deviceMemoryGb === null || deviceMemoryGb >= 8)
  ) {
    return "Expanded";
  }

  return "Balanced";
}

function effectiveProfile(
  configured: SafetyProfile,
  hardware: SafetyProfile
): SafetyProfile {
  return PROFILE_ORDER[configured] <= PROFILE_ORDER[hardware]
    ? configured
    : hardware;
}

function countDomNodes(): number {
  if (typeof document === "undefined") {
    return 0;
  }

  return document.getElementsByTagName("*").length;
}

function computeDomConstrainedCount(
  snapshot: RenderGuardSnapshot,
  desiredCount: number
): number {
  if (desiredCount <= 0) {
    return 0;
  }

  const headroom = snapshot.domSoftLimit - snapshot.domNodes;
  if (headroom <= 0) {
    return 0;
  }

  return Math.max(
    0,
    Math.min(desiredCount, Math.floor(headroom / ESTIMATED_DOM_NODES_PER_ITEM))
  );
}

function computePressureAdjustedCount(
  snapshot: RenderGuardSnapshot,
  desiredCount: number,
  elevatedRatio: number,
  criticalCap: number
): number {
  if (desiredCount <= 0) {
    return 0;
  }

  if (snapshot.runtimePressure === "critical") {
    return Math.max(0, Math.min(desiredCount, criticalCap));
  }

  if (snapshot.runtimePressure === "elevated") {
    return Math.max(1, Math.floor(desiredCount * elevatedRatio));
  }

  return desiredCount;
}

function scheduleRenderGuardBroadcast(): void {
  if (renderGuardTickScheduled) {
    return;
  }

  renderGuardTickScheduled = true;
  const flush = () => {
    renderGuardTickScheduled = false;
    const snapshot = getRenderGuardSnapshot();
    for (const listener of renderGuardListeners) {
      listener(snapshot);
    }
  };

  if (typeof window !== "undefined" && typeof window.requestAnimationFrame === "function") {
    window.requestAnimationFrame(() => flush());
    return;
  }

  queueMicrotask(flush);
}

function startRenderGuardObserver(): void {
  if (runtimeHealthCleanup === null) {
    runtimeHealthCleanup = subscribeRuntimeHealth(() => {
      scheduleRenderGuardBroadcast();
    });
  }

  if (
    renderGuardObserver !== null ||
    typeof MutationObserver === "undefined" ||
    typeof document === "undefined"
  ) {
    return;
  }

  renderGuardObserver = new MutationObserver(() => {
    scheduleRenderGuardBroadcast();
  });
  renderGuardObserver.observe(document.documentElement, {
    childList: true,
    subtree: true,
  });

  if (typeof window !== "undefined") {
    window.addEventListener("resize", scheduleRenderGuardBroadcast);
  }
}

function stopRenderGuardObserver(): void {
  renderGuardObserver?.disconnect();
  renderGuardObserver = null;

  runtimeHealthCleanup?.();
  runtimeHealthCleanup = null;

  if (typeof window !== "undefined") {
    window.removeEventListener("resize", scheduleRenderGuardBroadcast);
  }
}

export function getRenderGuardSnapshot(): RenderGuardSnapshot {
  const configuredProfile = getSettings().safety_profile;
  const hardwareProfile = detectHardwareProfile();
  const effective = effectiveProfile(configuredProfile, hardwareProfile);
  const budget = PROFILE_BUDGETS[effective];
  const domNodes = countDomNodes();
  const runtimeHealth = getRuntimeHealthSnapshot();

  let reason: RenderGuardSnapshot["reason"] = null;
  let blocked = false;
  if (domNodes >= budget.domSoftLimit) {
    reason = "dom-soft-limit";
    blocked = true;
  } else if (runtimeHealth.pressure === "critical") {
    reason = "runtime-pressure";
    blocked = true;
  } else if (domNodes >= Math.floor(budget.domSoftLimit * 0.88)) {
    reason = "dom-low-headroom";
  } else if (runtimeHealth.pressure === "elevated") {
    reason = "runtime-pressure";
  }

  return {
    configuredProfile,
    hardwareProfile,
    effectiveProfile: effective,
    ...getHardwareSignals(),
    domNodes,
    domSoftLimit: budget.domSoftLimit,
    initialRenderLimit: budget.initialRenderLimit,
    renderStep: budget.renderStep,
    iconPreloadBatch: budget.iconPreloadBatch,
    runtimePressure: runtimeHealth.pressure,
    effectsMode: runtimeHealth.effectsMode,
    runtimeReasons: runtimeHealth.reasons,
    runtimeSampleCount: runtimeHealth.sampleCount,
    blocked,
    reason,
  };
}

export function subscribeRenderGuard(listener: RenderGuardListener): () => void {
  renderGuardListeners.add(listener);
  startRenderGuardObserver();
  listener(getRenderGuardSnapshot());

  return () => {
    renderGuardListeners.delete(listener);
    if (renderGuardListeners.size === 0) {
      stopRenderGuardObserver();
    }
  };
}

export function getInitialVisibleCount(
  totalItems: number,
  snapshot: RenderGuardSnapshot = getRenderGuardSnapshot()
): number {
  const budget = PROFILE_BUDGETS[snapshot.effectiveProfile];
  const runtimeAdjustedLimit = computePressureAdjustedCount(
    snapshot,
    snapshot.blocked ? budget.emergencyRenderLimit : budget.initialRenderLimit,
    0.6,
    budget.emergencyRenderLimit
  );
  const safeLimit = computeDomConstrainedCount(snapshot, runtimeAdjustedLimit);
  return Math.min(totalItems, safeLimit);
}

export function getIconPreloadBatch(
  paths: Iterable<string>,
  snapshot: RenderGuardSnapshot = getRenderGuardSnapshot()
): string[] {
  const budget = PROFILE_BUDGETS[snapshot.effectiveProfile];
  const runtimeAdjustedBatch = computePressureAdjustedCount(
    snapshot,
    snapshot.blocked ? 0 : budget.iconPreloadBatch,
    0.5,
    0
  );
  const batch = computeDomConstrainedCount(snapshot, runtimeAdjustedBatch);

  if (batch <= 0) {
    return [];
  }

  const selectedPaths: string[] = [];
  for (const path of paths) {
    selectedPaths.push(path);
    if (selectedPaths.length >= batch) {
      break;
    }
  }

  return selectedPaths;
}

export function requestNextVisibleCount(
  currentVisibleCount: number,
  totalItems: number
): {
  nextVisibleCount: number;
  snapshot: RenderGuardSnapshot;
} {
  const snapshot = getRenderGuardSnapshot();

  if (totalItems <= currentVisibleCount) {
    return {
      nextVisibleCount: totalItems,
      snapshot,
    };
  }

  const budget = PROFILE_BUDGETS[snapshot.effectiveProfile];
  const runtimeAdjustedStep = computePressureAdjustedCount(
    snapshot,
    snapshot.blocked ? 0 : budget.renderStep,
    0.5,
    0
  );
  const adaptiveStep = computeDomConstrainedCount(snapshot, runtimeAdjustedStep);
  const blocked = adaptiveStep === 0;
  const reason =
    blocked && snapshot.reason === "runtime-pressure"
      ? "runtime-pressure"
      : blocked
        ? "dom-soft-limit"
        : snapshot.reason;

  return {
    nextVisibleCount:
      blocked
        ? currentVisibleCount
        : Math.min(totalItems, currentVisibleCount + adaptiveStep),
    snapshot: {
      ...snapshot,
      blocked,
      reason,
    },
  };
}
