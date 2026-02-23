<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { formatSize } from "./utils";

  interface Props {
    onSelect: (path: string) => void;
    onBack: () => void;
  }

  let { onSelect, onBack }: Props = $props();

  interface DriveInfo {
    path: string;
    total: number;
    free: number;
  }

  let drives: DriveInfo[] = $state([]);
  let selectedIndex: number = $state(0);
  let loading: boolean = $state(true);
  let error: string | null = $state(null);

  onMount(async () => {
    try {
      drives = await invoke<DriveInfo[]>("get_drives");
    } catch (e) {
      console.error("Failed to get drives:", e);
      error = String(e);
      drives = [];
    }
    loading = false;
  });

  function handleKeydown(e: KeyboardEvent) {
    switch (e.key) {
      case "ArrowUp":
      case "k":
        e.preventDefault();
        if (selectedIndex > 0) selectedIndex--;
        break;
      case "ArrowDown":
      case "j":
        e.preventDefault();
        if (selectedIndex < drives.length - 1) selectedIndex++;
        break;
      case "Enter":
        e.preventDefault();
        if (drives.length > 0) onSelect(drives[selectedIndex].path);
        break;
      case "q":
      case "Escape":
        e.preventDefault();
        onBack();
        break;
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="drive-picker">
  <div class="panel">
    <div class="panel-title">select volume</div>
    <div class="panel-content">
      {#if loading}
        <div class="loading">detecting volumes...</div>
      {:else if error}
        <div class="loading error">{error}</div>
      {:else if drives.length === 0}
        <div class="loading">no volumes found</div>
      {:else}
        <div class="drive-list">
          {#each drives as drive, i}
            {@const used = drive.total - drive.free}
            {@const pct =
              drive.total > 0 ? ((used / drive.total) * 100).toFixed(1) : "0.0"}
            <button
              class="drive-item"
              class:selected={i === selectedIndex}
              onmouseenter={() => (selectedIndex = i)}
              onclick={() => onSelect(drive.path)}
            >
              <span class="drive-path">{drive.path}</span>
              <span class="drive-stats">
                {formatSize(used)} / {formatSize(drive.total)}
                <span class="drive-pct">{pct}%</span>
              </span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
    <div class="panel-footer">enter scan · j/k nav · q back</div>
  </div>
</div>

<style>
  .drive-picker {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    width: 100%;
  }

  .panel {
    border: 1px solid var(--color-border);
    width: 60%;
    min-width: 400px;
    max-width: 700px;
    max-height: 70vh;
    display: flex;
    flex-direction: column;
  }

  .panel-title {
    padding: 4px 8px;
    color: var(--text-secondary);
    border-bottom: 1px solid var(--color-border);
    font-size: 12px;
  }

  .panel-content {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
  }

  .panel-footer {
    padding: 4px 8px;
    color: var(--text-muted);
    border-top: 1px solid var(--color-border);
    font-size: 12px;
  }

  .loading {
    padding: 16px;
    color: var(--text-secondary);
    text-align: center;
  }

  .loading.error {
    color: var(--color-error, #e06c75);
  }

  .drive-list {
    display: flex;
    flex-direction: column;
  }

  .drive-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 12px;
    background: none;
    border: none;
    cursor: pointer;
    font-family: inherit;
    font-size: 13px;
    color: var(--text-primary);
    text-align: left;
  }

  .drive-item.selected {
    background-color: var(--bg-selected);
    font-weight: bold;
  }

  .drive-path {
    color: var(--color-drive);
    font-weight: bold;
  }

  .drive-stats {
    color: var(--text-secondary);
    font-size: 12px;
  }

  .drive-pct {
    display: inline-block;
    width: 50px;
    text-align: right;
    color: var(--text-secondary);
  }
</style>
