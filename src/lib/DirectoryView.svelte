<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { formatSize } from "./utils";

  interface Props {
    onQuit: () => void;
    onDelete: (selections: Map<string, MarkedEntry[]>) => void;
  }

  let { onQuit, onDelete }: Props = $props();

  interface DirectoryEntry {
    name: string;
    size: number;
    is_dir: boolean;
    has_children: boolean;
  }

  interface DirectoryView {
    path: string;
    total_size: number;
    entries: DirectoryEntry[];
    item_count: number;
  }

  export interface MarkedEntry {
    name: string;
    size: number;
    is_dir: boolean;
  }

  // --- Column resize definitions ---

  const STORAGE_KEY = "disku-col-widths";
  const ORDER_STORAGE_KEY = "disku-col-order";
  const DRAG_THRESHOLD = 5;
  const COL_DEFS = [
    { key: "name", label: "Name", min: 80, defaultFrac: 0.3 },
    { key: "bar", label: "Bar", min: 60, defaultFrac: 0.4 },
    { key: "size", label: "Size", min: 50, defaultFrac: 0.18 },
    { key: "pct", label: "%", min: 40, defaultFrac: 0.12 },
  ];

  let colWidths: number[] = $state(COL_DEFS.map(() => 0));
  let colOrder: number[] = $state([0, 1, 2, 3]);
  let headerCellEls: (HTMLSpanElement | undefined)[] = $state([]);

  let resizing: {
    leftIndex: number;
    rightIndex: number;
    startX: number;
    startWidths: number[];
  } | null = $state(null);

  let dragging: {
    colDefIndex: number;
    startX: number;
    currentX: number;
    headerRects: DOMRect[];
    activated: boolean;
  } | null = $state(null);

  let gridCols = $derived(
    "36px " + colOrder.map((ci) => colWidths[ci] + "px").join(" "),
  );

  function getAvailableWidth(): number {
    if (!listEl) return 600;
    return listEl.clientWidth - 36; // subtract checkbox column
  }

  function initColWidths() {
    const available = getAvailableWidth();
    let fracs: number[] | null = null;
    try {
      const stored = localStorage.getItem(STORAGE_KEY);
      if (stored) {
        const parsed = JSON.parse(stored);
        if (
          Array.isArray(parsed) &&
          parsed.length === COL_DEFS.length &&
          parsed.every((v: unknown) => typeof v === "number" && v > 0 && v < 1)
        ) {
          fracs = parsed;
        }
      }
    } catch {}

    if (!fracs) {
      fracs = COL_DEFS.map((c) => c.defaultFrac);
    }

    const raw = fracs.map((f, i) => Math.max(COL_DEFS[i].min, f * available));
    const total = raw.reduce((a, b) => a + b, 0);
    colWidths = raw.map((w) => (w / total) * available);
  }

  function saveColWidths() {
    const available = getAvailableWidth();
    if (available <= 0) return;
    const fracs = colWidths.map((w) => w / available);
    localStorage.setItem(STORAGE_KEY, JSON.stringify(fracs));
  }

  function rescaleColWidths(newAvailable: number) {
    const adj = newAvailable - 36; // subtract checkbox column
    const oldTotal = colWidths.reduce((a, b) => a + b, 0);
    if (oldTotal <= 0) return;

    const scaled = colWidths.map((w, i) =>
      Math.max(COL_DEFS[i].min, (w / oldTotal) * adj),
    );
    const scaledTotal = scaled.reduce((a, b) => a + b, 0);
    const overshoot = scaledTotal - adj;
    if (Math.abs(overshoot) > 0.5) {
      scaled[1] = Math.max(COL_DEFS[1].min, scaled[1] - overshoot);
    }
    colWidths = scaled;
  }

  function onHandleMousedown(e: MouseEvent, visualHandleIndex: number) {
    e.preventDefault();
    resizing = {
      leftIndex: colOrder[visualHandleIndex],
      rightIndex: colOrder[visualHandleIndex + 1],
      startX: e.clientX,
      startWidths: [...colWidths],
    };
  }

  function onWindowMousemove(e: MouseEvent) {
    if (dragging) {
      onDragMousemove(e);
      return;
    }
    if (!resizing) return;
    const { leftIndex, rightIndex, startX, startWidths } = resizing;
    const delta = e.clientX - startX;
    const sumPair = startWidths[leftIndex] + startWidths[rightIndex];
    const newLeft = Math.min(
      Math.max(startWidths[leftIndex] + delta, COL_DEFS[leftIndex].min),
      sumPair - COL_DEFS[rightIndex].min,
    );
    const newRight = sumPair - newLeft;
    colWidths[leftIndex] = newLeft;
    colWidths[rightIndex] = newRight;
  }

  function onWindowMouseup() {
    if (dragging) {
      dragging = null;
      saveColOrder();
      return;
    }
    if (!resizing) return;
    resizing = null;
    saveColWidths();
  }

  function onHeaderMousedown(e: MouseEvent, colDefIndex: number) {
    e.preventDefault();
    const rects = headerCellEls.map(
      (el) => el?.getBoundingClientRect() ?? new DOMRect(),
    );
    dragging = {
      colDefIndex,
      startX: e.clientX,
      currentX: e.clientX,
      headerRects: rects,
      activated: false,
    };
  }

  function onDragMousemove(e: MouseEvent) {
    if (!dragging) return;
    dragging.currentX = e.clientX;

    if (!dragging.activated) {
      if (Math.abs(dragging.currentX - dragging.startX) < DRAG_THRESHOLD)
        return;
      dragging.activated = true;
    }

    const currentVisualPos = colOrder.indexOf(dragging.colDefIndex);
    const dropTarget = computeDropTarget(e.clientX, dragging.headerRects);

    if (dropTarget !== -1 && dropTarget !== currentVisualPos) {
      const newOrder = [...colOrder];
      newOrder.splice(currentVisualPos, 1);
      newOrder.splice(dropTarget, 0, dragging.colDefIndex);
      colOrder = newOrder;

      requestAnimationFrame(() => {
        if (dragging) {
          dragging.headerRects = headerCellEls.map(
            (el) => el?.getBoundingClientRect() ?? new DOMRect(),
          );
        }
      });
    }
  }

  function computeDropTarget(clientX: number, rects: DOMRect[]): number {
    for (let i = 0; i < rects.length; i++) {
      const rect = rects[i];
      const midpoint = rect.left + rect.width / 2;
      if (clientX < midpoint) return i;
    }
    return rects.length - 1;
  }

  function saveColOrder() {
    localStorage.setItem(ORDER_STORAGE_KEY, JSON.stringify(colOrder));
  }

  function initColOrder() {
    try {
      const stored = localStorage.getItem(ORDER_STORAGE_KEY);
      if (stored) {
        const parsed = JSON.parse(stored);
        if (
          Array.isArray(parsed) &&
          parsed.length === COL_DEFS.length &&
          parsed.every((v: unknown) => typeof v === "number") &&
          [...parsed].sort().every((v, i) => v === i)
        ) {
          colOrder = parsed;
          return;
        }
      }
    } catch {}
    colOrder = [0, 1, 2, 3];
  }

  // --- Existing state ---
  let navPath: number[] = $state([]);
  let sortBySize: boolean = $state(true);
  let view: DirectoryView | null = $state(null);
  let error: string | null = $state(null);
  let listEl: HTMLDivElement | undefined = $state();

  // --- Multi-select state ---
  let selectedIndices: Set<number> = $state(new Set());

  // --- Cross-directory selection persistence ---
  let globalSelections: Map<string, MarkedEntry[]> = $state(new Map());

  // Derived: total marked count across all directories
  let totalMarkedCount = $derived.by(() => {
    let count = 0;
    for (const entries of globalSelections.values()) {
      count += entries.length;
    }
    return count;
  });

  // Derived: total marked size across all directories
  let totalMarkedSize = $derived.by(() => {
    let total = 0;
    for (const entries of globalSelections.values()) {
      for (const e of entries) {
        total += e.size;
      }
    }
    return total;
  });

  // Count how many distinct paths have selections
  let markedPathCount = $derived.by(() => {
    let count = 0;
    for (const entries of globalSelections.values()) {
      if (entries.length > 0) count++;
    }
    return count;
  });

  function clearSelection() {
    selectedIndices = new Set();
    syncToGlobal();
  }

  function toggleSelect(idx: number) {
    const next = new Set(selectedIndices);
    if (next.has(idx)) {
      next.delete(idx);
    } else {
      next.add(idx);
    }
    selectedIndices = next;
    syncToGlobal();
  }

  function selectAll() {
    if (!view) return;
    const next = new Set<number>();
    for (let i = 0; i < view.entries.length; i++) {
      next.add(i);
    }
    selectedIndices = next;
    syncToGlobal();
  }

  // Sync current selectedIndices to globalSelections for the current view path
  function syncToGlobal() {
    if (!view) return;
    const path = view.path;
    if (selectedIndices.size === 0) {
      const next = new Map(globalSelections);
      next.delete(path);
      globalSelections = next;
    } else {
      const marked: MarkedEntry[] = [];
      for (const idx of selectedIndices) {
        if (idx < view.entries.length) {
          const e = view.entries[idx];
          marked.push({ name: e.name, size: e.size, is_dir: e.is_dir });
        }
      }
      const next = new Map(globalSelections);
      next.set(path, marked);
      globalSelections = next;
    }
  }

  // Restore selectedIndices from globalSelections for the current view
  function restoreFromGlobal() {
    if (!view) return;
    const path = view.path;
    const marked = globalSelections.get(path);
    if (!marked || marked.length === 0) {
      selectedIndices = new Set();
      return;
    }
    const markedNames = new Set(marked.map((m) => m.name));
    const next = new Set<number>();
    for (let i = 0; i < view.entries.length; i++) {
      if (markedNames.has(view.entries[i].name)) {
        next.add(i);
      }
    }
    selectedIndices = next;
  }

  // Save current selections before navigating away
  function saveBeforeNav() {
    syncToGlobal();
  }

  // Monotonic counter to guard against stale loadView responses
  let loadSeq = 0;

  function goBack() {
    if (navPath.length > 0) {
      saveBeforeNav();
      navPath = navPath.slice(0, -1);
      const seq = ++loadSeq;
      loadView(seq).then(() => {
        if (seq === loadSeq) restoreFromGlobal();
      });
    }
  }

  function navigateInto(idx: number) {
    if (!view) return;
    const entry = view.entries[idx];
    if (entry && entry.is_dir && entry.has_children) {
      saveBeforeNav();
      navPath = [...navPath, idx];
      const seq = ++loadSeq;
      loadView(seq).then(() => {
        if (seq === loadSeq) restoreFromGlobal();
      });
    }
  }

  function toggleSort() {
    sortBySize = !sortBySize;
    // Clear local indices but DON'T wipe globalSelections — re-map by name after reload
    selectedIndices = new Set();
    const seq = ++loadSeq;
    loadView(seq).then(() => {
      if (seq === loadSeq) restoreFromGlobal();
    });
  }

  function handleDeleteClick() {
    // Sync current selections, then pass globalSelections to parent
    syncToGlobal();
    if (totalMarkedCount === 0) return;
    onDelete(new Map(globalSelections));
  }

  async function loadView(seq?: number) {
    try {
      const result = await invoke<DirectoryView>("get_directory_view", {
        navPath,
        sortBySize: sortBySize,
      });
      // Only apply if this is still the latest request
      if (seq === undefined || seq === loadSeq) {
        view = result;
      }
    } catch (e) {
      console.error("Failed to load view:", e);
      if (seq === undefined || seq === loadSeq) {
        error = String(e);
      }
    }
  }

  // Called by App after deletion completes to clear selections and reload
  export function postDelete() {
    globalSelections = new Map();
    selectedIndices = new Set();
    navPath = [];
    loadView();
  }

  onMount(() => {
    loadView();
    initColOrder();
    requestAnimationFrame(() => {
      initColWidths();
    });
  });

  // ResizeObserver to rescale columns when container resizes
  $effect(() => {
    if (!listEl) return;
    const currentAvailable = listEl.clientWidth;
    // Skip when hidden (display: none gives clientWidth 0) to avoid infinite loop
    if (currentAvailable === 0) return;
    const currentTotal = colWidths.reduce((a, b) => a + b, 0);
    if (currentTotal > 0 && Math.abs(currentTotal - currentAvailable + 36) > 1) {
      rescaleColWidths(currentAvailable);
    }
    let prevWidth = currentAvailable;
    const ro = new ResizeObserver(() => {
      const newAvailable = listEl!.clientWidth;
      if (Math.abs(newAvailable - prevWidth) > 1) {
        rescaleColWidths(newAvailable);
        prevWidth = newAvailable;
      }
    });
    ro.observe(listEl);
    return () => ro.disconnect();
  });

  $effect(() => {
    if (!barEl) return;
    updateBarWidth();
    const ro = new ResizeObserver(() => updateBarWidth());
    ro.observe(barEl);
    return () => ro.disconnect();
  });

  let barEl: HTMLSpanElement | undefined = $derived(
    headerCellEls[colOrder.indexOf(1)],
  );
  let barWidth = $state(20);

  function updateBarWidth() {
    if (!barEl) return;
    const charWidth = 7;
    const style = getComputedStyle(barEl);
    const pl = parseFloat(style.paddingLeft) || 0;
    const pr = parseFloat(style.paddingRight) || 0;
    const inner = barEl.clientWidth - pl - pr;
    barWidth = Math.max(1, Math.ceil(inner / charWidth));
  }

  function makeBar(pct: number): string {
    const filled = Math.round((pct / 100) * barWidth);
    return "\u2588".repeat(filled) + "\u2591".repeat(barWidth - filled);
  }

  // Is the header checkbox in "all selected" state?
  let allSelected = $derived.by(() => {
    if (!view) return false;
    return view.entries.length > 0 && selectedIndices.size === view.entries.length;
  });
</script>

<svelte:window
  onmousemove={onWindowMousemove}
  onmouseup={onWindowMouseup}
/>

<div class="browser">
  {#if view}
    <div class="panel">
      <div class="panel-header">
        <span class="path">{view.path}</span>
        <span class="meta">
          {formatSize(view.total_size)} &middot; {view.item_count} items
        </span>
      </div>
      <div class="file-list-wrap" class:resizing={resizing !== null} class:reordering={dragging?.activated}>
        {#each [0, 1, 2] as hi}
          <div
            class="resize-handle"
            style:left="{36 + colOrder.slice(0, hi + 1).reduce((sum, ci) => sum + colWidths[ci], 0) - 6}px"
            onmousedown={(e) => onHandleMousedown(e, hi)}
            role="separator"
            aria-orientation="vertical"
            tabindex="-1"
          ></div>
        {/each}
        <div class="file-list" bind:this={listEl}>
          <div
            class="grid-header"
            style:grid-template-columns={gridCols}
          >
            <!-- Checkbox header -->
            <span class="col-checkbox" role="columnheader">
              <button
                class="checkbox-btn"
                onclick={() => { if (allSelected) { clearSelection(); } else { selectAll(); } }}
                aria-label={allSelected ? "Deselect all" : "Select all"}
              >
                {#if allSelected}
                  <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                    <rect x="1" y="1" width="12" height="12" rx="2" fill="var(--color-red)" stroke="var(--color-red)" stroke-width="1.5"/>
                    <path d="M4 7L6 9L10 5" stroke="white" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                  </svg>
                {:else if selectedIndices.size > 0}
                  <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                    <rect x="1" y="1" width="12" height="12" rx="2" fill="var(--color-red)" stroke="var(--color-red)" stroke-width="1.5"/>
                    <line x1="4" y1="7" x2="10" y2="7" stroke="white" stroke-width="1.5" stroke-linecap="round"/>
                  </svg>
                {:else}
                  <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                    <rect x="1" y="1" width="12" height="12" rx="2" stroke="var(--text-muted)" stroke-width="1.5"/>
                  </svg>
                {/if}
              </button>
            </span>
            {#each colOrder as ci, vi}
              <span
                class="col-{COL_DEFS[ci].key}"
                bind:this={headerCellEls[vi]}
                class:drag-source={dragging?.activated && dragging.colDefIndex === ci}
                onmousedown={(e) => onHeaderMousedown(e, ci)}
                role="columnheader"
                tabindex="-1"
              >
                {COL_DEFS[ci].label}
              </span>
            {/each}
          </div>
          {#each view.entries as entry, i}
            {@const pct =
              view.total_size > 0
                ? (entry.size / view.total_size) * 100
                : 0}
            {@const isMarked = selectedIndices.has(i)}
            <button
              class="entry"
              class:marked={isMarked}
              style:grid-template-columns={gridCols}
              onclick={() => {
                if (entry.is_dir && entry.has_children) {
                  navigateInto(i);
                }
              }}
            >
              <!-- Checkbox cell -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <span
                class="col-checkbox"
                onclick={(e) => {
                  e.stopPropagation();
                  toggleSelect(i);
                }}
              >
                {#if isMarked}
                  <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                    <rect x="1" y="1" width="12" height="12" rx="2" fill="var(--color-red)" stroke="var(--color-red)" stroke-width="1.5"/>
                    <path d="M4 7L6 9L10 5" stroke="white" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                  </svg>
                {:else}
                  <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                    <rect x="1" y="1" width="12" height="12" rx="2" stroke="var(--text-muted)" stroke-width="1.5"/>
                  </svg>
                {/if}
              </span>
              {#each colOrder as ci}
                {#if COL_DEFS[ci].key === "name"}
                  <span class="col-name" class:dir={entry.is_dir}>{entry.name}</span>
                {:else if COL_DEFS[ci].key === "bar"}
                  <span class="col-bar">{makeBar(pct)}</span>
                {:else if COL_DEFS[ci].key === "size"}
                  <span class="col-size">{formatSize(entry.size)}</span>
                {:else if COL_DEFS[ci].key === "pct"}
                  <span class="col-pct">{pct.toFixed(1)}%</span>
                {/if}
              {/each}
            </button>
          {/each}
        </div>
      </div>

      <!-- Selection status bar -->
      {#if totalMarkedCount > 0}
        <div class="status-bar">
          <span class="status-count">
            {#if markedPathCount > 1}
              {totalMarkedCount} items marked across {markedPathCount} paths
            {:else}
              {totalMarkedCount} item{totalMarkedCount === 1 ? '' : 's'} marked for deletion
            {/if}
          </span>
          <span class="status-size">{formatSize(totalMarkedSize)} will be freed</span>
        </div>
      {/if}

      <!-- Action bar -->
      <div class="action-bar">
        <div class="action-left">
          <button class="action-btn" onclick={toggleSort}>
            <svg width="12" height="12" viewBox="0 0 12 12">
              <path d="M2 3h8M2 6h5M2 9h3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
            </svg>
            Sort: {sortBySize ? "Size" : "Name"}
          </button>
          {#if view && view.entries.length > 0}
            <button class="action-link" onclick={selectAll}>Select All</button>
            <span class="action-sep">&middot;</span>
            <button class="action-link" onclick={clearSelection}>Clear</button>
          {/if}
        </div>
        <div class="action-right">
          {#if navPath.length > 0}
            <button class="action-btn" onclick={goBack}>
              <svg width="12" height="12" viewBox="0 0 12 12">
                <path d="M7 2L3 6L7 10" stroke="currentColor" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
              Back
            </button>
          {:else}
            <button class="action-btn" onclick={onQuit}>
              <svg width="12" height="12" viewBox="0 0 12 12">
                <path d="M7 2L3 6L7 10" stroke="currentColor" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
              Volumes
            </button>
          {/if}
          {#if totalMarkedCount > 0}
            <button class="delete-btn" onclick={handleDeleteClick}>
              <svg width="12" height="12" viewBox="0 0 12 12">
                <path d="M3 3h6M4 3V2h4v1M2 3h8M3 3v7h6V3M5 5v3M7 5v3" stroke="currentColor" fill="none" stroke-linecap="round"/>
              </svg>
              Delete {totalMarkedCount} Item{totalMarkedCount === 1 ? '' : 's'}
            </button>
          {/if}
        </div>
      </div>
    </div>
  {:else if error}
    <div class="loading error">{error}</div>
  {:else}
    <div class="loading">loading...</div>
  {/if}
</div>

<style>
  .browser {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    width: 100%;
    padding: 20px;
  }

  .panel {
    border: 2px solid var(--color-border);
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 4px 8px;
    color: var(--text-secondary);
    border-bottom: 2px solid var(--color-border);
    font-size: 12px;
    flex-shrink: 0;
  }

  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
  }

  .meta {
    white-space: nowrap;
    margin-left: 16px;
    flex-shrink: 0;
  }

  .file-list-wrap {
    position: relative;
    flex: 1;
    min-height: 0;
  }

  .file-list-wrap.resizing,
  .file-list-wrap.reordering {
    user-select: none;
  }

  .file-list-wrap.reordering {
    cursor: grabbing;
  }

  .grid-header > span:not(.col-checkbox) {
    cursor: grab;
  }

  .grid-header > .col-bar {
    color: var(--color-size);
  }

  .file-list-wrap.reordering .grid-header > span {
    cursor: grabbing;
  }

  .drag-source {
    opacity: 0.5;
    background: var(--bg-hover, rgba(255, 255, 255, 0.05));
  }

  .file-list {
    height: 100%;
    overflow-y: auto;
  }

  .grid-header,
  .entry {
    display: grid;
    width: 100%;
    font-family: inherit;
    font-size: 13px;
    color: var(--text-primary);
    text-align: left;
  }

  .grid-header {
    position: sticky;
    top: 0;
    background: var(--bg-primary, #1e1e1e);
    font-size: 11px;
    color: var(--text-muted);
    border-bottom: 2px solid var(--color-border);
    z-index: 1;
  }

  .grid-header > span {
    padding: 3px 4px;
    border-right: 1px solid var(--color-border);
  }

  .grid-header > span:last-child {
    border-right: none;
  }

  .col-checkbox {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0 !important;
    border-right: 1px solid var(--color-border) !important;
  }

  .checkbox-btn {
    background: none;
    border: none;
    cursor: pointer;
    padding: 2px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .resize-handle {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 12px;
    cursor: col-resize;
    z-index: 3;
  }

  .resize-handle::after {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    left: 5px;
    width: 2px;
    transition: opacity 0.15s;
    opacity: 0;
    background: var(--color-accent);
  }

  .resize-handle:hover::after,
  .file-list-wrap.resizing > .resize-handle::after {
    opacity: 0.5;
  }

  .entry {
    background: none;
    border: none;
    border-bottom: 1px solid color-mix(in srgb, var(--color-border) 40%, transparent);
    cursor: pointer;
    padding: 0;
  }

  .entry > span {
    padding: 2px 4px;
    border-right: 1px solid color-mix(in srgb, var(--color-border) 40%, transparent);
    display: flex;
    align-items: center;
  }

  .entry > span:last-child {
    border-right: none;
  }

  .entry:hover {
    background-color: var(--bg-hover);
  }

  /* Marked (selected for deletion) styling */
  .entry.marked {
    background-color: color-mix(in srgb, var(--color-red) 15%, transparent);
  }

  .entry.marked:hover {
    background-color: color-mix(in srgb, var(--color-red) 22%, transparent);
  }

  .entry.marked .col-name {
    color: var(--color-red);
  }

  .entry.marked .col-size,
  .entry.marked .col-pct {
    color: #d09090;
  }

  .entry.marked .col-bar {
    color: var(--color-red);
  }

  .col-bar {
    overflow: hidden;
    white-space: nowrap;
    color: var(--color-accent, #5899f0);
    letter-spacing: -1px;
    padding: 2px 10px !important;
  }

  .col-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-file);
  }

  .col-name.dir {
    color: var(--color-dir);
  }

  .col-size {
    justify-content: flex-end;
    color: var(--color-size);
  }

  .col-pct {
    justify-content: flex-end;
    color: var(--color-size);
  }

  /* Selection status bar — Paper: 8Z-0 */
  .status-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 4px 8px;
    background: color-mix(in srgb, var(--color-red) 8%, transparent);
    border-top: 2px solid var(--color-border);
    font-size: 12px;
    flex-shrink: 0;
  }

  .status-count {
    color: var(--color-red);
    font-weight: 700;
  }

  .status-size {
    color: var(--color-red);
  }

  /* Action bar — Paper: B7-0 */
  .action-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 8px;
    border-top: 2px solid var(--color-border);
    font-size: 11px;
    flex-shrink: 0;
  }

  .action-left,
  .action-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  /* Bordered buttons (Sort, Back) — Paper: B9-0, I4-0 */
  .action-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    background: none;
    border: 1px solid var(--color-border);
    border-radius: 3px;
    color: var(--text-secondary);
    font-family: inherit;
    font-size: 11px;
    padding: 3px 10px;
    cursor: pointer;
    white-space: nowrap;
  }

  .action-btn:hover {
    color: var(--text-primary);
    border-color: var(--text-secondary);
  }

  /* Delete button — Paper: BH-0 */
  .delete-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    background: var(--color-red);
    border: none;
    border-radius: 3px;
    color: #fff;
    font-family: inherit;
    font-size: 11px;
    font-weight: 700;
    padding: 4px 14px;
    cursor: pointer;
    white-space: nowrap;
  }

  .delete-btn:hover {
    opacity: 0.85;
  }

  /* Text links (Select All, Clear) — Paper: BE-0, BG-0 */
  .action-link {
    background: none;
    border: none;
    color: var(--color-accent);
    font-family: inherit;
    font-size: 11px;
    cursor: pointer;
    padding: 0;
  }

  .action-link:hover {
    color: var(--text-primary);
  }

  .action-sep {
    color: var(--text-muted);
    font-size: 11px;
  }

  .loading {
    color: var(--text-secondary);
  }

  .loading.error {
    color: var(--color-error, #e06c75);
  }
</style>
