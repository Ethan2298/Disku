<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  interface Props {
    onQuit: () => void;
  }

  let { onQuit }: Props = $props();

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
    colOrder.map((ci) => colWidths[ci] + "px").join(" "),
  );

  function getAvailableWidth(): number {
    if (!listEl) return 600;
    return listEl.clientWidth;
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
    const oldTotal = colWidths.reduce((a, b) => a + b, 0);
    if (oldTotal <= 0) return;

    const scaled = colWidths.map((w, i) =>
      Math.max(COL_DEFS[i].min, (w / oldTotal) * newAvailable),
    );
    const scaledTotal = scaled.reduce((a, b) => a + b, 0);
    const overshoot = scaledTotal - newAvailable;
    // Distribute overshoot/undershoot to bar column (index 1)
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
  let selectedIndex: number = $state(0);
  let sortBySize: boolean = $state(true);
  let view: DirectoryView | null = $state(null);
  let listEl: HTMLDivElement | undefined = $state();

  async function loadView() {
    try {
      view = await invoke<DirectoryView>("get_directory_view", {
        navPath,
        sortBySize: sortBySize,
      });
      if (view && selectedIndex >= view.entries.length) {
        selectedIndex = Math.max(0, view.entries.length - 1);
      }
    } catch (e) {
      console.error("Failed to load view:", e);
    }
  }

  onMount(() => {
    loadView();
    initColOrder();
    // Wait for DOM, then measure and init column widths
    requestAnimationFrame(() => {
      initColWidths();
    });
  });

  // ResizeObserver to rescale columns when container resizes
  $effect(() => {
    if (!listEl) return;
    const currentAvailable = listEl.clientWidth;
    const currentTotal = colWidths.reduce((a, b) => a + b, 0);
    if (currentTotal > 0 && Math.abs(currentTotal - currentAvailable) > 1) {
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
    return "█".repeat(filled) + "░".repeat(barWidth - filled);
  }

  function formatSize(bytes: number): string {
    const KB = 1024;
    const MB = KB * 1024;
    const GB = MB * 1024;
    const TB = GB * 1024;

    if (bytes >= TB) return (bytes / TB).toFixed(1) + " TB";
    if (bytes >= GB) return (bytes / GB).toFixed(1) + " GB";
    if (bytes >= MB) return (bytes / MB).toFixed(1) + " MB";
    if (bytes >= KB) return (bytes / KB).toFixed(1) + " KB";
    return bytes + " B";
  }

  function scrollToSelected() {
    if (!listEl) return;
    const items = listEl.querySelectorAll(".entry");
    const item = items[selectedIndex];
    if (item) {
      item.scrollIntoView({ block: "nearest" });
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!view) return;

    switch (e.key) {
      case "ArrowUp":
      case "k":
        e.preventDefault();
        if (selectedIndex > 0) {
          selectedIndex--;
          scrollToSelected();
        }
        break;
      case "ArrowDown":
      case "j":
        e.preventDefault();
        if (selectedIndex < view.entries.length - 1) {
          selectedIndex++;
          scrollToSelected();
        }
        break;
      case "Enter": {
        e.preventDefault();
        const entry = view.entries[selectedIndex];
        if (entry && entry.is_dir && entry.has_children) {
          navPath = [...navPath, selectedIndex];
          selectedIndex = 0;
          loadView();
        }
        break;
      }
      case "Backspace":
        e.preventDefault();
        if (navPath.length > 0) {
          navPath = navPath.slice(0, -1);
          selectedIndex = 0;
          loadView();
        }
        break;
      case "s":
        e.preventDefault();
        sortBySize = !sortBySize;
        loadView();
        break;
      case "q":
        e.preventDefault();
        onQuit();
        break;
    }
  }
</script>

<svelte:window
  onkeydown={handleKeydown}
  onmousemove={onWindowMousemove}
  onmouseup={onWindowMouseup}
/>

<div class="browser">
  {#if view}
    <div class="panel">
      <div class="panel-header">
        <span class="path">{view.path}</span>
        <span class="meta">
          {formatSize(view.total_size)} · {view.item_count} items · [{sortBySize
            ? "size"
            : "name"}]
        </span>
      </div>
      <div class="file-list-wrap" class:resizing={resizing !== null} class:reordering={dragging?.activated}>
        {#each [0, 1, 2] as hi}
          <div
            class="resize-handle"
            style:left="{colOrder.slice(0, hi + 1).reduce((sum, ci) => sum + colWidths[ci], 0) - 2}px"
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
            <button
              class="entry"
              class:selected={i === selectedIndex}
              style:grid-template-columns={gridCols}
              onmouseenter={() => (selectedIndex = i)}
              onclick={() => {
                selectedIndex = i;
                if (entry.is_dir && entry.has_children) {
                  navPath = [...navPath, i];
                  selectedIndex = 0;
                  loadView();
                }
              }}
            >
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
      <div class="panel-footer">
        <span class="key">enter</span>
        <span class="desc">open</span>
        <span class="key">bksp</span>
        <span class="desc">back</span>
        <span class="key">j/k</span>
        <span class="desc">nav</span>
        <span class="key">s</span>
        <span class="desc">sort</span>
        <span class="key">q</span>
        <span class="desc">start</span>
      </div>
    </div>
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
    border: 1px solid var(--color-border);
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
    border-bottom: 1px solid var(--color-border);
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

  .grid-header > span {
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
    border-bottom: 1px solid var(--color-border);
    z-index: 1;
  }

  .grid-header > span {
    padding: 3px 4px;
    border-right: 1px solid var(--color-border);
  }

  .grid-header > span:last-child {
    border-right: none;
  }

  .resize-handle {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 5px;
    cursor: col-resize;
    z-index: 3;
  }

  .resize-handle:hover,
  .file-list-wrap.resizing > .resize-handle {
    background: color-mix(in srgb, var(--color-accent) 30%, transparent);
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

  .entry.selected {
    background-color: var(--bg-selected);
    font-weight: bold;
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

  .panel-footer {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    border-top: 1px solid var(--color-border);
    font-size: 12px;
    flex-shrink: 0;
  }

  .key {
    color: var(--color-accent);
  }

  .desc {
    color: var(--text-muted);
    margin-right: 8px;
  }

  .loading {
    color: var(--text-secondary);
  }
</style>
