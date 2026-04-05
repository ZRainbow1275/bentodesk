import { getSettings } from "../stores/settings";
import type { SafetyProfile } from "../types/settings";
import type { MemoryInfo } from "../types/system";
import { getMemoryUsage } from "./ipc";

export type RuntimePressureLevel = "stable" | "elevated" | "critical";
export type RuntimeEffectsMode = "full" | "reduced" | "minimal";

interface RuntimeBudget {
  jsHeapSoftLimitMb: number;
  jsHeapCriticalLimitMb: number;
  privateUsageSoftLimitMb: number;
  privateUsageCriticalLimitMb: number;
  workingSetSoftLimitMb: number;
  workingSetCriticalLimitMb: number;
  slowFrameThresholdMs: number;
  elevatedSlowFrameRatio: number;
  criticalSlowFrameRatio: number;
}

interface ChromiumPerformanceMemory {
  usedJSHeapSize?: number;
  totalJSHeapSize?: number;
  jsHeapSizeLimit?: number;
}

interface ChromiumPerformance extends Performance {
  memory?: ChromiumPerformanceMemory;
}

type RuntimeHealthListener = (snapshot: RuntimeHealthSnapshot) => void;

export interface RuntimeHealthSnapshot {
  configuredProfile: SafetyProfile;
  hardwareConcurrency: number | null;
  deviceMemoryGb: number | null;
  sampleCount: number;
  jsHeapUsedBytes: number | null;
  jsHeapLimitBytes: number | null;
  processWorkingSetBytes: number | null;
  processPrivateBytes: number | null;
  processPagefileBytes: number | null;
  frameP95Ms: number | null;
  slowFrameRatio: number;
  pressure: RuntimePressureLevel;
  effectsMode: RuntimeEffectsMode;
  baselineEffectsMode: RuntimeEffectsMode;
  reasons: string[];
  workingSetSeriesMb: number[];
  privateUsageSeriesMb: number[];
  jsHeapSeriesMb: number[];
}

const MAX_SERIES_POINTS = 36;
const FRAME_WINDOW_SIZE = 120;
const MEMORY_POLL_INTERVAL_MS = 5000;
const FRAME_BROADCAST_INTERVAL_MS = 1500;

const PROFILE_BUDGETS: Record<SafetyProfile, RuntimeBudget> = {
  Conservative: {
    jsHeapSoftLimitMb: 96,
    jsHeapCriticalLimitMb: 160,
    privateUsageSoftLimitMb: 220,
    privateUsageCriticalLimitMb: 320,
    workingSetSoftLimitMb: 280,
    workingSetCriticalLimitMb: 420,
    slowFrameThresholdMs: 28,
    elevatedSlowFrameRatio: 0.2,
    criticalSlowFrameRatio: 0.35,
  },
  Balanced: {
    jsHeapSoftLimitMb: 144,
    jsHeapCriticalLimitMb: 224,
    privateUsageSoftLimitMb: 320,
    privateUsageCriticalLimitMb: 480,
    workingSetSoftLimitMb: 380,
    workingSetCriticalLimitMb: 560,
    slowFrameThresholdMs: 32,
    elevatedSlowFrameRatio: 0.18,
    criticalSlowFrameRatio: 0.32,
  },
  Expanded: {
    jsHeapSoftLimitMb: 192,
    jsHeapCriticalLimitMb: 320,
    privateUsageSoftLimitMb: 420,
    privateUsageCriticalLimitMb: 640,
    workingSetSoftLimitMb: 500,
    workingSetCriticalLimitMb: 760,
    slowFrameThresholdMs: 36,
    elevatedSlowFrameRatio: 0.16,
    criticalSlowFrameRatio: 0.28,
  },
};

const runtimeHealthListeners = new Set<RuntimeHealthListener>();

let monitoringActive = false;
let memoryPollHandle: number | null = null;
let animationFrameHandle: number | null = null;
let frameBroadcastTimestamp = 0;
let lastFrameTimestamp: number | null = null;
let memoryPollInFlight = false;

let latestMemoryInfo: MemoryInfo | null = null;
let latestJsHeapUsedBytes: number | null = null;
let latestJsHeapLimitBytes: number | null = null;
let sampleCount = 0;

const frameDurationsMs: number[] = [];
const workingSetSeriesMb: number[] = [];
const privateUsageSeriesMb: number[] = [];
const jsHeapSeriesMb: number[] = [];

function resetRuntimeHealthState(): void {
  latestMemoryInfo = null;
  latestJsHeapUsedBytes = null;
  latestJsHeapLimitBytes = null;
  sampleCount = 0;

  frameDurationsMs.splice(0, frameDurationsMs.length);
  workingSetSeriesMb.splice(0, workingSetSeriesMb.length);
  privateUsageSeriesMb.splice(0, privateUsageSeriesMb.length);
  jsHeapSeriesMb.splice(0, jsHeapSeriesMb.length);
}

function toMegabytes(bytes: number): number {
  return bytes / (1024 * 1024);
}

function toBytes(megabytes: number): number {
  return megabytes * 1024 * 1024;
}

function pushSeriesPoint(series: number[], value: number | null): void {
  if (value === null) {
    return;
  }

  series.push(Number(value.toFixed(1)));
  if (series.length > MAX_SERIES_POINTS) {
    series.splice(0, series.length - MAX_SERIES_POINTS);
  }
}

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

function detectBaselineEffectsMode(): RuntimeEffectsMode {
  const { hardwareConcurrency, deviceMemoryGb } = getHardwareSignals();
  const prefersReducedMotion =
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  if (
    prefersReducedMotion ||
    (hardwareConcurrency !== null && hardwareConcurrency <= 4) ||
    (deviceMemoryGb !== null && deviceMemoryGb <= 4)
  ) {
    return "reduced";
  }

  return "full";
}

function readPerformanceMemory(): {
  usedBytes: number | null;
  limitBytes: number | null;
} {
  if (typeof performance === "undefined") {
    return {
      usedBytes: null,
      limitBytes: null,
    };
  }

  const performanceWithMemory = performance as ChromiumPerformance;
  const memory = performanceWithMemory.memory;
  if (!memory) {
    return {
      usedBytes: null,
      limitBytes: null,
    };
  }

  const usedBytes =
    typeof memory.usedJSHeapSize === "number" ? memory.usedJSHeapSize : null;
  const limitBytes =
    typeof memory.jsHeapSizeLimit === "number" ? memory.jsHeapSizeLimit : null;

  return {
    usedBytes,
    limitBytes,
  };
}

function getCurrentBudget(): RuntimeBudget {
  return PROFILE_BUDGETS[getSettings().safety_profile];
}

function getFrameStats(
  slowFrameThresholdMs: number
): {
  frameP95Ms: number | null;
  slowFrameRatio: number;
} {
  if (frameDurationsMs.length === 0) {
    return {
      frameP95Ms: null,
      slowFrameRatio: 0,
    };
  }

  const sorted = [...frameDurationsMs].sort((left, right) => left - right);
  const p95Index = Math.min(
    sorted.length - 1,
    Math.max(0, Math.ceil(sorted.length * 0.95) - 1)
  );
  const slowFrameCount = frameDurationsMs.filter(
    (duration) => duration >= slowFrameThresholdMs
  ).length;

  return {
    frameP95Ms: Number(sorted[p95Index].toFixed(1)),
    slowFrameRatio: slowFrameCount / frameDurationsMs.length,
  };
}

function evaluatePressure(
  budget: RuntimeBudget,
  frameP95Ms: number | null,
  slowFrameRatio: number
): {
  pressure: RuntimePressureLevel;
  reasons: string[];
} {
  const reasons: string[] = [];
  let pressure: RuntimePressureLevel = "stable";

  const escalate = (candidate: RuntimePressureLevel, reason: string): void => {
    if (candidate === "critical" || pressure === "stable") {
      pressure = candidate;
    }
    reasons.push(reason);
  };

  if (latestJsHeapUsedBytes !== null) {
    if (latestJsHeapUsedBytes >= toBytes(budget.jsHeapCriticalLimitMb)) {
      escalate("critical", "js-heap-critical");
    } else if (latestJsHeapUsedBytes >= toBytes(budget.jsHeapSoftLimitMb)) {
      escalate("elevated", "js-heap-soft-limit");
    }
  }

  if (latestMemoryInfo !== null) {
    if (latestMemoryInfo.private_usage_bytes >= toBytes(budget.privateUsageCriticalLimitMb)) {
      escalate("critical", "process-private-critical");
    } else if (
      latestMemoryInfo.private_usage_bytes >= toBytes(budget.privateUsageSoftLimitMb)
    ) {
      escalate("elevated", "process-private-soft-limit");
    }

    if (latestMemoryInfo.working_set_bytes >= toBytes(budget.workingSetCriticalLimitMb)) {
      escalate("critical", "working-set-critical");
    } else if (
      latestMemoryInfo.working_set_bytes >= toBytes(budget.workingSetSoftLimitMb)
    ) {
      escalate("elevated", "working-set-soft-limit");
    }
  }

  if (
    frameP95Ms !== null &&
    (slowFrameRatio >= budget.criticalSlowFrameRatio ||
      frameP95Ms >= budget.slowFrameThresholdMs * 1.5)
  ) {
    escalate("critical", "slow-frames-critical");
  } else if (
    frameP95Ms !== null &&
    (slowFrameRatio >= budget.elevatedSlowFrameRatio ||
      frameP95Ms >= budget.slowFrameThresholdMs)
  ) {
    escalate("elevated", "slow-frames-elevated");
  }

  return {
    pressure,
    reasons,
  };
}

function resolveEffectsMode(
  baselineEffectsMode: RuntimeEffectsMode,
  pressure: RuntimePressureLevel
): RuntimeEffectsMode {
  if (pressure === "critical") {
    return "minimal";
  }

  if (pressure === "elevated") {
    return baselineEffectsMode === "full" ? "reduced" : "minimal";
  }

  return baselineEffectsMode;
}

function applyRuntimeEffects(snapshot: RuntimeHealthSnapshot): void {
  if (typeof document === "undefined") {
    return;
  }

  document.documentElement.dataset.runtimeEffects = snapshot.effectsMode;
  document.documentElement.dataset.runtimePressure = snapshot.pressure;
}

function buildRuntimeHealthSnapshot(): RuntimeHealthSnapshot {
  const budget = getCurrentBudget();
  const baselineEffectsMode = detectBaselineEffectsMode();
  const { frameP95Ms, slowFrameRatio } = getFrameStats(
    budget.slowFrameThresholdMs
  );
  const { pressure, reasons } = evaluatePressure(
    budget,
    frameP95Ms,
    slowFrameRatio
  );

  return {
    configuredProfile: getSettings().safety_profile,
    ...getHardwareSignals(),
    sampleCount,
    jsHeapUsedBytes: latestJsHeapUsedBytes,
    jsHeapLimitBytes: latestJsHeapLimitBytes,
    processWorkingSetBytes: latestMemoryInfo?.working_set_bytes ?? null,
    processPrivateBytes: latestMemoryInfo?.private_usage_bytes ?? null,
    processPagefileBytes: latestMemoryInfo?.pagefile_usage_bytes ?? null,
    frameP95Ms,
    slowFrameRatio,
    pressure,
    effectsMode: resolveEffectsMode(baselineEffectsMode, pressure),
    baselineEffectsMode,
    reasons,
    workingSetSeriesMb: [...workingSetSeriesMb],
    privateUsageSeriesMb: [...privateUsageSeriesMb],
    jsHeapSeriesMb: [...jsHeapSeriesMb],
  };
}

function broadcastRuntimeHealth(): void {
  const snapshot = buildRuntimeHealthSnapshot();
  applyRuntimeEffects(snapshot);

  for (const listener of runtimeHealthListeners) {
    listener(snapshot);
  }
}

function queueFrameSample(timestamp: number): void {
  if (!monitoringActive) {
    return;
  }

  if (typeof document !== "undefined" && document.visibilityState === "hidden") {
    lastFrameTimestamp = null;
  } else if (lastFrameTimestamp !== null) {
    const duration = timestamp - lastFrameTimestamp;
    frameDurationsMs.push(duration);
    if (frameDurationsMs.length > FRAME_WINDOW_SIZE) {
      frameDurationsMs.splice(0, frameDurationsMs.length - FRAME_WINDOW_SIZE);
    }
  }

  lastFrameTimestamp = timestamp;

  if (timestamp - frameBroadcastTimestamp >= FRAME_BROADCAST_INTERVAL_MS) {
    frameBroadcastTimestamp = timestamp;
    broadcastRuntimeHealth();
  }

  if (typeof window !== "undefined" && typeof window.requestAnimationFrame === "function") {
    animationFrameHandle = window.requestAnimationFrame(queueFrameSample);
  }
}

async function sampleRuntimeMemory(): Promise<void> {
  if (memoryPollInFlight) {
    return;
  }

  memoryPollInFlight = true;
  try {
    const heapMetrics = readPerformanceMemory();
    latestJsHeapUsedBytes = heapMetrics.usedBytes;
    latestJsHeapLimitBytes = heapMetrics.limitBytes;

    const memoryInfo = await getMemoryUsage();
    latestMemoryInfo = memoryInfo;
    sampleCount += 1;

    pushSeriesPoint(
      workingSetSeriesMb,
      toMegabytes(memoryInfo.working_set_bytes)
    );
    pushSeriesPoint(
      privateUsageSeriesMb,
      toMegabytes(memoryInfo.private_usage_bytes)
    );
    pushSeriesPoint(
      jsHeapSeriesMb,
      heapMetrics.usedBytes === null ? null : toMegabytes(heapMetrics.usedBytes)
    );
  } catch (error) {
    console.error("Failed to sample runtime health", error);
  } finally {
    memoryPollInFlight = false;
    broadcastRuntimeHealth();
  }
}

export function startRuntimeHealthMonitoring(): void {
  if (monitoringActive) {
    return;
  }

  monitoringActive = true;
  frameBroadcastTimestamp = 0;
  lastFrameTimestamp = null;
  resetRuntimeHealthState();

  if (typeof window !== "undefined") {
    memoryPollHandle = window.setInterval(() => {
      void sampleRuntimeMemory();
    }, MEMORY_POLL_INTERVAL_MS);

    if (typeof window.requestAnimationFrame === "function") {
      animationFrameHandle = window.requestAnimationFrame(queueFrameSample);
    }
  }

  void sampleRuntimeMemory();
  broadcastRuntimeHealth();
}

export function stopRuntimeHealthMonitoring(): void {
  monitoringActive = false;

  if (memoryPollHandle !== null && typeof window !== "undefined") {
    window.clearInterval(memoryPollHandle);
  }
  memoryPollHandle = null;

  if (animationFrameHandle !== null && typeof window !== "undefined") {
    window.cancelAnimationFrame(animationFrameHandle);
  }
  animationFrameHandle = null;
  lastFrameTimestamp = null;
  frameBroadcastTimestamp = 0;
  memoryPollInFlight = false;
  resetRuntimeHealthState();

  if (typeof document !== "undefined") {
    delete document.documentElement.dataset.runtimeEffects;
    delete document.documentElement.dataset.runtimePressure;
  }
}

export function getRuntimeHealthSnapshot(): RuntimeHealthSnapshot {
  return buildRuntimeHealthSnapshot();
}

export function subscribeRuntimeHealth(
  listener: RuntimeHealthListener
): () => void {
  runtimeHealthListeners.add(listener);
  listener(getRuntimeHealthSnapshot());

  return () => {
    runtimeHealthListeners.delete(listener);
  };
}
