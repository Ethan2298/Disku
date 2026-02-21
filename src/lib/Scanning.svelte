<script lang="ts">
  import { invoke, Channel } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  interface Props {
    path: string;
    filesScanned: number;
    dirsScanned: number;
    onProgress: (files: number, dirs: number, errors: number) => void;
    onComplete: () => void;
  }

  let { path, filesScanned, dirsScanned, onProgress, onComplete }: Props =
    $props();

  let recentPaths: string[] = $state([]);
  const MAX_VISIBLE = 16;

  let spinnerFrame = $state(0);
  const spinnerChars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

  function shortenPath(fullPath: string): string {
    if (fullPath.startsWith(path)) {
      const rel = fullPath.slice(path.length);
      return rel.startsWith("/") ? rel.slice(1) : rel;
    }
    return fullPath;
  }

  onMount(() => {
    const spinnerInterval = setInterval(() => {
      spinnerFrame = (spinnerFrame + 1) % spinnerChars.length;
    }, 80);

    const onEvent = new Channel<{
      kind: string;
      files_scanned?: number;
      dirs_scanned?: number;
      errors?: number;
      current_path?: string;
    }>();

    onEvent.onmessage = (event) => {
      if (event.kind === "Progress") {
        onProgress(
          event.files_scanned ?? 0,
          event.dirs_scanned ?? 0,
          event.errors ?? 0,
        );
        if (event.current_path) {
          const shortened = shortenPath(event.current_path);
          if (shortened && shortened !== recentPaths[0]) {
            recentPaths = [shortened, ...recentPaths].slice(0, MAX_VISIBLE);
          }
        }
      } else if (event.kind === "Complete") {
        onComplete();
      }
    };

    invoke("start_scan", { path, onEvent }).catch((e) => {
      console.error("Scan failed:", e);
    });

    return () => {
      clearInterval(spinnerInterval);
    };
  });
</script>

<div class="scanning">
  <div class="panel">
    <div class="panel-title">disku</div>
    <div class="panel-content">
      <div class="status-row">
        <span class="spinner">{spinnerChars[spinnerFrame]}</span>
        {#if filesScanned === 0 && dirsScanned === 0}
          <span class="status initializing">initializing</span>
        {:else}
          <span class="status active">scanning</span>
        {/if}
      </div>
      <p class="detail">
        {filesScanned.toLocaleString()} files &middot; {dirsScanned.toLocaleString()}
        folders &middot; {path}
      </p>
    </div>
    <div class="file-feed">
      {#each recentPaths as p, i}
        <div class="feed-line" style:opacity={1 - i * (0.7 / MAX_VISIBLE)}>
          <span class="feed-path">{p}</span>
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .scanning {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    width: 100%;
  }

  .panel {
    border: 1px solid var(--color-border);
    width: 80%;
    min-width: 360px;
    max-width: 700px;
    display: flex;
    flex-direction: column;
    max-height: 70vh;
  }

  .panel-title {
    padding: 4px 8px;
    color: var(--text-secondary);
    border-bottom: 1px solid var(--color-border);
    font-size: 12px;
  }

  .panel-content {
    padding: 16px 16px 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    border-bottom: 1px solid var(--color-border);
  }

  .status-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .spinner {
    color: var(--color-accent);
    font-size: 16px;
    width: 16px;
    text-align: center;
  }

  .status {
    font-weight: bold;
    font-size: 14px;
  }

  .status.initializing {
    color: var(--text-secondary);
  }

  .status.active {
    color: var(--color-accent);
  }

  .detail {
    color: var(--text-secondary);
    font-size: 12px;
    padding-left: 24px;
  }

  .file-feed {
    flex: 1;
    overflow: hidden;
    padding: 8px 16px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .feed-line {
    display: flex;
    align-items: center;
    font-size: 12px;
    height: 20px;
    min-height: 20px;
    color: var(--text-secondary);
    overflow: hidden;
  }

  .feed-path {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    direction: rtl;
    text-align: left;
  }
</style>
