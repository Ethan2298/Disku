<script lang="ts">
  import { invoke, Channel } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  interface Props {
    path: string;
    filesScanned: number;
    onProgress: (files: number, errors: number) => void;
    onComplete: () => void;
  }

  let { path, filesScanned, onProgress, onComplete }: Props = $props();

  let dots: string = $state("");

  onMount(() => {
    // Animate dots
    const interval = setInterval(() => {
      dots = dots.length >= 3 ? "" : dots + ".";
    }, 400);

    // Start scan with progress channel
    const onEvent = new Channel<{
      kind: string;
      files_scanned?: number;
      errors?: number;
    }>();

    onEvent.onmessage = (event) => {
      if (event.kind === "Progress") {
        onProgress(event.files_scanned ?? 0, event.errors ?? 0);
      } else if (event.kind === "Complete") {
        onComplete();
      }
    };

    invoke("start_scan", { path, onEvent }).catch((e) => {
      console.error("Scan failed:", e);
    });

    return () => {
      clearInterval(interval);
    };
  });
</script>

<div class="scanning">
  <div class="panel">
    <div class="panel-title">disku</div>
    <div class="panel-content">
      {#if filesScanned === 0}
        <p class="status initializing">initializing{dots}</p>
        <p class="detail">reading filesystem...</p>
      {:else}
        <p class="status active">scanning{dots}</p>
        <p class="detail">{filesScanned.toLocaleString()} files</p>
      {/if}
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
    width: 44%;
    min-width: 300px;
    max-width: 500px;
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
    padding: 32px 24px;
    display: flex;
    flex-direction: column;
    gap: 8px;
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
    font-size: 13px;
  }
</style>
