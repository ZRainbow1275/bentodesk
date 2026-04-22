/**
 * Stacks store — read-only derivations over `zonesStore`.
 *
 * We never mutate these directly: the source of truth is each zone's
 * `stack_id` / `stack_order` field persisted by the backend. This store
 * offers reactive groupings + helpers for the Theme D render path.
 */
import { createMemo } from "solid-js";
import type { BentoZone } from "../types/zone";
import { zonesStore, loadZones } from "./zones";
import {
  stackZones as ipcStackZones,
  unstackZones as ipcUnstackZones,
  reorderStack as ipcReorderStack,
} from "../services/ipc";

/**
 * `stack_id -> zones[]`, sorted by `stack_order` ascending (bottom → top).
 * Includes only stacks with >= 2 members — single-member "stacks" are treated
 * as free-standing zones by the render path.
 */
export const stackMap = createMemo(() => {
  const map = new Map<string, BentoZone[]>();
  for (const z of zonesStore.zones) {
    const sid = z.stack_id;
    if (!sid) continue;
    const bucket = map.get(sid);
    if (bucket) bucket.push(z);
    else map.set(sid, [z]);
  }
  for (const arr of map.values()) {
    arr.sort((a, b) => (a.stack_order ?? 0) - (b.stack_order ?? 0));
  }
  // Drop degenerate singletons (can happen transiently during unstack).
  for (const [k, v] of map) {
    if (v.length < 2) map.delete(k);
  }
  return map;
});

/**
 * `zone_id -> stack_id` fast lookup. Use this to decide whether a zone is
 * inside a stack during render. Returns undefined for free-standing zones.
 */
export const zoneStackId = createMemo(() => {
  const idx = new Map<string, string>();
  for (const [sid, zones] of stackMap()) {
    for (const z of zones) idx.set(z.id, sid);
  }
  return idx;
});

/** The single top-most zone of a given stack, or `undefined` if empty. */
export function getStackTop(stackId: string): BentoZone | undefined {
  const arr = stackMap().get(stackId);
  if (!arr || arr.length === 0) return undefined;
  return arr[arr.length - 1];
}

export function getStackMembers(stackId: string): BentoZone[] {
  return stackMap().get(stackId) ?? [];
}

/** IPC wrappers keep consumers from importing `services/ipc` everywhere. */
export async function stackZonesAction(zoneIds: string[]): Promise<string | null> {
  if (zoneIds.length < 2) return null;
  try {
    const stackId = await ipcStackZones(zoneIds);
    // Reload layout so stack_id / stack_order reflect server-side truth.
    await loadZones();
    return stackId;
  } catch {
    return null;
  }
}

export async function unstackZonesAction(stackId: string): Promise<boolean> {
  try {
    await ipcUnstackZones(stackId);
    await loadZones();
    return true;
  } catch {
    return false;
  }
}

export async function reorderStackAction(
  stackId: string,
  zoneId: string,
  newOrder: number,
): Promise<boolean> {
  try {
    await ipcReorderStack(stackId, zoneId, newOrder);
    await loadZones();
    return true;
  } catch {
    return false;
  }
}

export async function detachZoneFromStackAction(
  stackId: string,
  zoneId: string,
): Promise<boolean> {
  const members = getStackMembers(stackId);
  if (members.length < 2) return false;
  const remainingIds = members
    .filter((zone) => zone.id !== zoneId)
    .map((zone) => zone.id);
  try {
    await ipcUnstackZones(stackId);
    if (remainingIds.length >= 2) {
      await ipcStackZones(remainingIds);
    }
    await loadZones();
    return true;
  } catch {
    return false;
  }
}
