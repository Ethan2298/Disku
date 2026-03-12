<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { formatSize } from "./utils";

  interface Props {
    onSelect: (path: string) => void;
    onScanDirectory: () => void;
  }

  let { onSelect, onScanDirectory }: Props = $props();

  interface DriveInfo {
    path: string;
    total: number;
    free: number;
  }

  let drives: DriveInfo[] = $state([]);
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
</script>

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
          {#each drives as drive}
            {@const used = drive.total - drive.free}
            {@const pct =
              drive.total > 0 ? ((used / drive.total) * 100).toFixed(1) : "0.0"}
            <button
              class="drive-item"
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
    <div class="panel-footer">
      <button class="back-btn" onclick={onScanDirectory}>
        <svg width="12" height="12" viewBox="0 0 12 12">
          <path d="M1 2h4l1.5 2H11v6H1V2z" stroke="currentColor" stroke-width="1" fill="none" stroke-linejoin="round"/>
        </svg>
        Scan Directory
      </button>
    </div>
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
    border: 2px solid var(--color-border);
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
    border-bottom: 2px solid var(--color-border);
    font-size: 12px;
  }

  .panel-content {
    flex: 1;
    overflow-y: auto;
    padding: 0;
  }

  .panel-footer {
    display: flex;
    justify-content: flex-end;
    padding: 4px 8px;
    border-top: 2px solid var(--color-border);
    font-size: 12px;
  }

  .back-btn {
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
  }

  .back-btn:hover {
    color: var(--text-primary);
    border-color: var(--text-secondary);
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
    border-bottom: 1px solid var(--color-border);
    cursor: pointer;
    font-family: inherit;
    font-size: 13px;
    color: var(--text-primary);
    text-align: left;
  }

  .drive-item:hover {
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
