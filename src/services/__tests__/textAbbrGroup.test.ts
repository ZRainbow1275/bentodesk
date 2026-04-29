/**
 * Tests for the v8 FontGroup primitive that powers font-uniformity in
 * BentoPanel and StackTray. We focus on the pure registration / aggregation
 * surface (`createFontGroup`) — the `useTextAbbrGroup` Solid composable
 * shares its measurement pipeline with `useTextAbbr` (which is covered by
 * textAbbr.test.ts), and the only group-specific behaviour is "expose the
 * group's `groupFontSize` instead of the local fit when a context provider
 * is mounted", which is type-checked by the source.
 *
 * Solid's `createMemo` requires a reactive owner (`createRoot`) — we use
 * `createRoot` to read accessors synchronously without triggering "computations
 * created outside a `createRoot` will never be disposed" warnings.
 */
import { describe, it, expect } from "vitest";
import { createRoot, createSignal } from "solid-js";
import { createFontGroup } from "../textAbbrGroup";
import { MIN_FONT_SIZE_PX } from "../textAbbr";

describe("createFontGroup", () => {
  it("emits the minimum size across all registered members", () => {
    createRoot((dispose) => {
      const group = createFontGroup(11);
      group.register("a", () => 13);
      group.register("b", () => 10);
      group.register("c", () => 8);
      expect(group.groupFontSize()).toBe(8);
      dispose();
    });
  });

  it("recomputes the minimum when a member unregisters", () => {
    createRoot((dispose) => {
      const group = createFontGroup(11);
      group.register("a", () => 13);
      group.register("b", () => 10);
      group.register("c", () => 8);
      group.unregister("c");
      expect(group.groupFontSize()).toBe(10);
      dispose();
    });
  });

  it("returns the default size when the group is empty", () => {
    createRoot((dispose) => {
      const group = createFontGroup(11);
      expect(group.groupFontSize()).toBe(11);
      dispose();
    });
  });

  it("returns the default size after every member unregisters", () => {
    // Sanity-check the empty-after-mutation path; previously `members().size`
    // could end up >0 if `unregister` mutated in place — guards the immutable
    // Map-replacement contract that drives Solid signal change detection.
    createRoot((dispose) => {
      const group = createFontGroup(13);
      group.register("only", () => 9);
      expect(group.groupFontSize()).toBe(9);
      group.unregister("only");
      expect(group.groupFontSize()).toBe(13);
      dispose();
    });
  });

  it("clamps the emitted size to MIN_FONT_SIZE_PX", () => {
    // A pathological member that demands a sub-MIN size must not pull the
    // whole panel below the readability floor.
    createRoot((dispose) => {
      const group = createFontGroup(11);
      group.register("tiny", () => 3);
      expect(group.groupFontSize()).toBe(MIN_FONT_SIZE_PX);
      dispose();
    });
  });

  it("treats register() with an existing id as a replace (no duplicates)", () => {
    // The same hook id calling register twice (e.g. on prop-driven re-keying)
    // must not leave a stale accessor in the map.
    createRoot((dispose) => {
      const group = createFontGroup(11);
      group.register("a", () => 13);
      group.register("a", () => 9);
      expect(group.groupFontSize()).toBe(9);
      dispose();
    });
  });

  it("reacts to changes in a member's needed size", () => {
    // The group's groupFontSize is a createMemo; updating a member's signal
    // must propagate so the group re-emits the new minimum.
    createRoot((dispose) => {
      const group = createFontGroup(11);
      const [aSize, setASize] = createSignal(12);
      const [bSize] = createSignal(10);
      group.register("a", aSize);
      group.register("b", bSize);
      expect(group.groupFontSize()).toBe(10);
      setASize(8);
      expect(group.groupFontSize()).toBe(8);
      dispose();
    });
  });

  it("ignores unregister calls for unknown ids", () => {
    createRoot((dispose) => {
      const group = createFontGroup(11);
      group.register("a", () => 9);
      group.unregister("does-not-exist");
      expect(group.groupFontSize()).toBe(9);
      dispose();
    });
  });

  it("ignores unmeasured members (Infinity neededSize) in the minimum", () => {
    // v8.3 regression guard: in the release WebView, a member's mount-time
    // `clientWidth` can be 0 for several frames; `useTextAbbrGroup` reports
    // `+Infinity` during that window so the group's minimum reflects only
    // already-measured members. Without this filter, every panel mount
    // would briefly pin groupFontSize at the bootstrap default (13/11),
    // and if any single member happened to keep clientWidth 0 forever
    // (off-screen tabs, hidden via display:none, slow flex distribution)
    // the whole column would render at the unshrunken default.
    createRoot((dispose) => {
      const group = createFontGroup(11);
      // Mix: one already-measured short name (10), one slow / unmeasured
      // member (+Infinity), one already-measured long name (8).
      group.register("measured-short", () => 10);
      group.register("unmeasured", () => Number.POSITIVE_INFINITY);
      group.register("measured-long", () => 8);
      expect(group.groupFontSize()).toBe(8);
      dispose();
    });
  });

  it("returns the default size when every member is unmeasured", () => {
    // Edge case: panel just mounted, no member has measured yet. The min
    // over {+Infinity, +Infinity, +Infinity} is still +Infinity, so the
    // group must fall through to the default rather than emit Infinity
    // (which would render as `style="font-size: Infinitypx"` — a CSS
    // invalid value the WebView would silently ignore, leaving the user-
    // agent default style in place and producing a different ragged look).
    createRoot((dispose) => {
      const group = createFontGroup(11);
      group.register("a", () => Number.POSITIVE_INFINITY);
      group.register("b", () => Number.POSITIVE_INFINITY);
      expect(group.groupFontSize()).toBe(11);
      dispose();
    });
  });
});
