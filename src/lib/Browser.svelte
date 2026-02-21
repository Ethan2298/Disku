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
  });

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

<svelte:window onkeydown={handleKeydown} />

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
      <div class="file-list" bind:this={listEl}>
        {#each view.entries as entry, i}
          {@const pct =
            view.total_size > 0
              ? ((entry.size / view.total_size) * 100).toFixed(1)
              : "0.0"}
          <button
            class="entry"
            class:selected={i === selectedIndex}
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
            <span class="icon" class:dir={entry.is_dir}
              >{entry.is_dir ? "+" : " "}</span
            >
            <span class="name" class:dir={entry.is_dir}>{entry.name}</span>
            <span class="size">{formatSize(entry.size)}</span>
            <span class="pct">{pct}%</span>
          </button>
        {/each}
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
    padding: 24px;
  }

  .panel {
    border: 1px solid var(--color-border);
    width: 88%;
    height: 90%;
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

  .file-list {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
  }

  .entry {
    display: flex;
    align-items: center;
    width: 100%;
    padding: 2px 0;
    background: none;
    border: none;
    cursor: pointer;
    font-family: inherit;
    font-size: 13px;
    color: var(--text-primary);
    text-align: left;
  }

  .entry.selected {
    background-color: var(--bg-selected);
    font-weight: bold;
  }

  .icon {
    width: 24px;
    text-align: center;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .icon.dir {
    color: var(--color-dir);
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-file);
  }

  .name.dir {
    color: var(--color-dir);
  }

  .size {
    width: 80px;
    text-align: right;
    color: var(--color-size);
    flex-shrink: 0;
    padding-right: 4px;
  }

  .pct {
    width: 55px;
    text-align: right;
    color: var(--color-pct);
    flex-shrink: 0;
    padding-right: 8px;
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
