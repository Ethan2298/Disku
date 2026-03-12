<script lang="ts">
  import { formatSize } from "./utils";
  import type { MarkedEntry } from "./DirectoryView.svelte";

  interface Props {
    selections: Map<string, MarkedEntry[]>;
    onCancel: () => void;
    onConfirm: (selections: Map<string, MarkedEntry[]>) => Promise<void>;
  }

  let { selections: initialSelections, onCancel, onConfirm }: Props = $props();

  // Local mutable copy so user can remove items before confirming
  let selections: Map<string, MarkedEntry[]> = $state(new Map(initialSelections));
  let deleting: boolean = $state(false);

  let totalCount = $derived.by(() => {
    let count = 0;
    for (const entries of selections.values()) {
      count += entries.length;
    }
    return count;
  });

  let totalSize = $derived.by(() => {
    let size = 0;
    for (const entries of selections.values()) {
      for (const e of entries) {
        size += e.size;
      }
    }
    return size;
  });

  let pathCount = $derived.by(() => {
    let count = 0;
    for (const entries of selections.values()) {
      if (entries.length > 0) count++;
    }
    return count;
  });

  function removeItem(dirPath: string, name: string) {
    const next = new Map(selections);
    const entries = next.get(dirPath);
    if (entries) {
      const filtered = entries.filter((e) => e.name !== name);
      if (filtered.length === 0) {
        next.delete(dirPath);
      } else {
        next.set(dirPath, filtered);
      }
    }
    selections = next;

    // Auto-return if all items removed
    if (selections.size === 0) {
      onCancel();
    }
  }

  async function handleConfirm() {
    deleting = true;
    try {
      await onConfirm(selections);
    } finally {
      deleting = false;
    }
  }
</script>

<div class="confirm-page">
  <div class="confirm-panel">
    <div class="confirm-header">confirm deletion</div>

    <div class="confirm-body">
      <p class="confirm-question">
        Permanently delete {totalCount} item{totalCount === 1 ? '' : 's'}{pathCount > 1 ? ` across ${pathCount} paths` : ''}?
      </p>

      <div class="confirm-list">
        {#each [...selections.entries()] as [dirPath, entries]}
          {#if entries.length > 0}
            <div class="dir-group">
              <div class="dir-path">{dirPath}</div>
              {#each entries as entry}
                <div class="dir-entry">
                  <span class="entry-name">{entry.name}</span>
                  <span class="entry-size">{formatSize(entry.size)}</span>
                  <button
                    class="remove-btn"
                    onclick={() => removeItem(dirPath, entry.name)}
                    aria-label="Remove {entry.name}"
                  >
                    <svg width="10" height="10" viewBox="0 0 10 10">
                      <path d="M2.5 2.5L7.5 7.5M7.5 2.5L2.5 7.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
                    </svg>
                    Remove
                  </button>
                </div>
              {/each}
            </div>
          {/if}
        {/each}
      </div>

      <div class="confirm-total">
        <span class="total-label">Total:</span>
        <span class="total-value">{formatSize(totalSize)}</span>
      </div>
    </div>

    <div class="confirm-actions">
      <button class="btn btn-cancel" onclick={onCancel} disabled={deleting}>Cancel</button>
      <button class="btn btn-delete" onclick={handleConfirm} disabled={deleting}>
        {#if deleting}
          Deleting...
        {:else}
          <svg width="12" height="12" viewBox="0 0 12 12">
            <path d="M3 3h6M4 3V2h4v1M2 3h8M3 3v7h6V3M5 5v3M7 5v3" stroke="currentColor" fill="none" stroke-linecap="round"/>
          </svg>
          Permanently Delete
        {/if}
      </button>
    </div>
  </div>
</div>

<style>
  .confirm-page {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    width: 100%;
    padding: 40px;
  }

  .confirm-panel {
    border: 2px solid var(--color-border);
    width: 100%;
    max-width: 520px;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
  }

  /* Paper: FM-0 — salmon tinted header with salmon bottom border */
  .confirm-header {
    padding: 4px 8px;
    color: var(--color-red);
    font-size: 12px;
    font-weight: 700;
    background: color-mix(in srgb, var(--color-red) 12%, transparent);
    border-bottom: 2px solid var(--color-red);
  }

  .confirm-body {
    flex: 1;
    overflow-y: auto;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .confirm-question {
    font-size: 13px;
    font-weight: 700;
    color: var(--text-primary);
  }

  .confirm-list {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .dir-group {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  /* Paper: GV-0 — muted dir path */
  .dir-path {
    font-size: 11px;
    color: var(--text-muted);
    padding: 2px 0;
  }

  .dir-entry {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 1px 16px;
    font-size: 12px;
  }

  .entry-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-primary);
    font-weight: 700;
  }

  .entry-size {
    color: var(--text-secondary);
    font-size: 12px;
    flex-shrink: 0;
  }

  /* Paper: HM-0 — salmon bordered remove button */
  .remove-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    background: none;
    border: 1px solid var(--color-red);
    border-radius: 3px;
    color: var(--color-red);
    font-family: inherit;
    font-size: 10px;
    cursor: pointer;
    padding: 2px 8px;
    flex-shrink: 0;
    margin-left: auto;
  }

  .remove-btn:hover {
    background: color-mix(in srgb, var(--color-red) 15%, transparent);
  }

  /* Paper: GJ-0 — total line with subtle border */
  .confirm-total {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 13px;
    border-top: 1px solid color-mix(in srgb, var(--color-border) 40%, transparent);
    padding-top: 4px;
  }

  .total-label {
    color: var(--text-secondary);
  }

  .total-value {
    color: var(--color-red);
    font-weight: 700;
  }

  /* Paper: GM-0 — salmon top border footer */
  .confirm-actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    padding: 10px 16px;
    border-top: 2px solid var(--color-red);
  }

  .btn {
    font-family: inherit;
    font-size: 12px;
    cursor: pointer;
    display: flex;
    align-items: center;
    border-radius: 3px;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  /* Paper: GN-0 — bordered cancel button */
  .btn-cancel {
    background: none;
    border: 1px solid var(--color-border);
    color: var(--text-secondary);
    padding: 4px 14px;
  }

  .btn-cancel:hover:not(:disabled) {
    color: var(--text-primary);
    border-color: var(--text-secondary);
  }

  /* Paper: GP-0 — filled salmon delete button */
  .btn-delete {
    background: var(--color-red);
    border: none;
    color: #fff;
    font-weight: 700;
    padding: 4px 14px;
    gap: 4px;
  }

  .btn-delete:hover:not(:disabled) {
    opacity: 0.85;
  }
</style>
